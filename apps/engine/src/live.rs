//! Live milestone scheduler. One heap, not one task per token.
//! Phase 7.2 had snapshots on events but never ticked FeatureEngine.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use chrono::Utc;

use crate::candidate::{CandidateEngine, CandidateInput, CandidateState};
use crate::domain::{CanonicalEvent, Chain, Launchpad, QualityStatus};
use crate::error::Result;
use crate::features::engine::{FeatureEngine, FeatureInput};
use crate::ingest::evm::pons_curve::{
    classify_paper_failure, execution_quality_label, CurveReadErrorKind, PonsCurveReader,
};
use crate::lab::pons_exp::{
    arm_id_for, parse_exit_policy, prospective_entry_eligible, EXIT_POLICIES,
};
use crate::metrics::DiscoveryMetrics;
use crate::sim::descriptive::DescriptiveTokenOutcome;
use crate::sim::models::SimConfig;
use crate::sim::policy::exit_policy;
use crate::sim::position::{PositionManager, SimulatedPosition};
use crate::sim::types::{ExecutionStatus, PositionStatus};
use crate::state::clock::StateClock;
use crate::state::pons_curve::overlay_snapshot;
use crate::state::schedule::SnapshotSchedule;
use crate::state::{PonsCurveState, StateEngine, TokenKey, TokenStateSnapshot};
use crate::storage::postgres::PostgresStore;
use crate::storage::EventStore;
use crate::strategy::{smoke_decide, ProspectivePolicy, StrategyContext};

#[derive(Debug, Default)]
pub struct LiveMilestoneScheduler {
    heap: BinaryHeap<Reverse<(i64, String, i64)>>,
    queued: HashSet<(String, i64)>,
    registered: HashSet<String>,
    pub scheduled: u64,
    pub popped: u64,
}

fn key_id(k: &TokenKey) -> String {
    format!("{}|{}", k.chain.as_str(), k.token)
}

impl LiveMilestoneScheduler {
    pub fn register(&mut self, key: &TokenKey, discovered_ms: i64, schedule: &SnapshotSchedule) {
        let kid = key_id(key);
        if !self.registered.insert(kid) {
            return;
        }
        for age in &schedule.milestones_ms {
            let due = discovered_ms.saturating_add(*age);
            let id = (key_id(key), *age);
            if self.queued.insert(id) {
                self.heap.push(Reverse((
                    due,
                    format!("{}|{}", key.chain.as_str(), key.token),
                    *age,
                )));
                self.scheduled += 1;
                DiscoveryMetrics::live_milestone_due();
            }
        }
    }

    pub fn pop_due(&mut self, now_ms: i64) -> Vec<(TokenKey, i64, i64)> {
        let mut out = Vec::new();
        while let Some(Reverse((due, id, age))) = self.heap.peek().cloned() {
            if due > now_ms {
                break;
            }
            self.heap.pop();
            self.queued.remove(&(id.clone(), age));
            self.popped += 1;
            let mut parts = id.splitn(2, '|');
            let chain = Chain::parse(parts.next().unwrap_or("solana")).unwrap_or(Chain::Solana);
            let token = parts.next().unwrap_or("").to_string();
            let lateness = now_ms.saturating_sub(due);
            DiscoveryMetrics::live_milestone_lateness_ms(lateness);
            if lateness > 2_000 {
                DiscoveryMetrics::live_milestone_missed();
            }
            out.push((TokenKey::new(chain, token), age, lateness));
        }
        out
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
}

pub struct PendingPaper {
    pub chain: Chain,
    pub token: String,
    pub launchpad: Launchpad,
    pub decision_time: chrono::DateTime<Utc>,
    pub feature_id: Option<i64>,
    pub sec_id: Option<i64>,
    pub quality: QualityStatus,
    pub snaps: Vec<TokenStateSnapshot>,
    pub curve: Option<String>,
    pub attempts: u32,
    pub experiment_id: Option<String>,
    pub entry_policy: String,
    pub exit_policy: String,
    pub arm_id: String,
    pub alpha_research_valid: bool,
}

pub struct LiveResearchRuntime {
    pub candidate: CandidateEngine,
    pub cand_state: HashMap<(Chain, String), CandidateState>,
    pub first_eligible: HashMap<(Chain, String), chrono::DateTime<Utc>>,
    pub entered: HashSet<(Chain, String)>,
    pub positions: Vec<SimulatedPosition>,
    pub pending: Vec<PendingPaper>,
    pub cfg: SimConfig,
    pub paper: bool,
    pub exp001: bool,
    pub experiment_id: Option<String>,
    pub exp_started_at: Option<chrono::DateTime<Utc>>,
    pub token_discovered: HashMap<(Chain, String), chrono::DateTime<Utc>>,
    pub next_pos: i64,
    pub smoke_orders: u64,
    pub smoke_fills: u64,
    pub smoke_partial: u64,
    pub smoke_failed: u64,
    pub research_obs: HashMap<&'static str, u64>,
    pub research_signals: HashMap<&'static str, u64>,
    pub recovered: u64,
    pub token_curve: HashMap<(Chain, String), String>,
    pub fail_reasons: HashMap<&'static str, u64>,
    pub entered_arms: HashSet<(Chain, String, String)>,
}

impl LiveResearchRuntime {
    pub fn new(paper: bool) -> Self {
        Self::new_mode(paper, false)
    }

    pub fn new_mode(paper: bool, exp001: bool) -> Self {
        Self {
            candidate: CandidateEngine::default_research(),
            cand_state: HashMap::new(),
            first_eligible: HashMap::new(),
            entered: HashSet::new(),
            positions: Vec::new(),
            pending: Vec::new(),
            cfg: SimConfig::research_default(),
            paper,
            exp001,
            experiment_id: None,
            exp_started_at: None,
            token_discovered: HashMap::new(),
            next_pos: 1,
            smoke_orders: 0,
            smoke_fills: 0,
            smoke_partial: 0,
            smoke_failed: 0,
            research_obs: HashMap::new(),
            research_signals: HashMap::new(),
            recovered: 0,
            token_curve: HashMap::new(),
            fail_reasons: HashMap::new(),
            entered_arms: HashSet::new(),
        }
    }
}

pub async fn persist_and_research<S: EventStore>(
    store: &S,
    snap: &mut TokenStateSnapshot,
    history: &[TokenStateSnapshot],
    runtime: &mut LiveResearchRuntime,
    pg: Option<&PostgresStore>,
) -> Result<()> {
    let sid = store.insert_snapshot(snap).await.ok();
    if let Some(id) = sid {
        snap.id = Some(id);
        let _ = store
            .upsert_current_state(
                snap.chain,
                &snap.token_address,
                Some(id),
                snap.lifecycle_state.as_str(),
                Some(snap.snapshot_time),
                snap.as_of_event_id.as_deref(),
                snap.data_quality,
            )
            .await;
        DiscoveryMetrics::snapshots_persisted(1);
    }
    if snap.snapshot_kind != crate::state::snapshot::SnapshotKind::Milestone
        && snap.snapshot_kind != crate::state::snapshot::SnapshotKind::Periodic
        && snap.snapshot_kind != crate::state::snapshot::SnapshotKind::Lifecycle
    {
        return Ok(());
    }
    let sec = store
        .latest_assessment(snap.chain, &snap.token_address)
        .await
        .ok()
        .flatten();
    let input = FeatureInput::from_history(snap, history, sec.as_ref());
    let mut vec = FeatureEngine::compute(input);
    vec.created_at = Utc::now();
    DiscoveryMetrics::live_feature_vector(snap.chain, snap.launchpad);
    if let Ok(id) = store.insert_feature_vector(&vec).await {
        vec.id = Some(id);
    }
    let key = (snap.chain, snap.token_address.clone());
    let current = runtime
        .cand_state
        .get(&key)
        .copied()
        .unwrap_or(CandidateState::Discovered);
    let cin = CandidateInput {
        chain: snap.chain,
        token: &snap.token_address,
        launchpad: snap.launchpad,
        age_ms: snap.age_ms,
        as_of_time: snap.snapshot_time,
        snapshot_id: snap.id,
        security: sec.as_ref(),
        features: Some(&vec),
        buy_count: snap.buy_count_total,
        unique_buyers: snap.unique_buyers_total,
        trade_count: snap.buy_count_total.saturating_add(snap.sell_count_total),
        lifecycle: snap.lifecycle_state,
        time_since_last_trade_ms: snap.wallet.last_trade_age_ms,
    };
    let steps = runtime.candidate.step_until_stable(current, &cin);
    if let Some(last) = steps.last() {
        runtime.cand_state.insert(key.clone(), last.to_state);
        if last.to_state == CandidateState::Eligible {
            runtime
                .first_eligible
                .entry(key.clone())
                .or_insert(snap.snapshot_time);
        }
    }
    for t in &steps {
        let _ = store.insert_candidate_transition(t).await;
    }
    let cand = runtime
        .cand_state
        .get(&key)
        .copied()
        .unwrap_or(CandidateState::Discovered);
    let verdict = sec.as_ref().map(|s| s.verdict);
    let ctx = StrategyContext {
        features: Some(&vec),
        candidate: cand,
        security: verdict,
        first_eligible_at: runtime.first_eligible.get(&key).copied(),
        now: snap.snapshot_time,
        token: &snap.token_address,
        seed: 1,
    };
    for p in ProspectivePolicy::all() {
        *runtime.research_obs.entry(p.id()).or_insert(0) += 1;
        let d = p.decide(&ctx);
        if d.enter {
            *runtime.research_signals.entry(p.id()).or_insert(0) += 1;
            DiscoveryMetrics::paper_signal(p.id());
        }
        if let Some(pg) = pg {
            let _ = pg
                .insert_prospective_signal(
                    snap.chain,
                    &snap.token_address,
                    snap.launchpad,
                    p.id(),
                    snap.snapshot_time,
                    d.enter,
                    d.reason,
                    runtime.exp001 && d.enter && cand == CandidateState::Eligible,
                    vec.id,
                    sec.as_ref().and_then(|s| s.id),
                    cand.as_str(),
                    &runtime.cfg.quote_notional,
                )
                .await;
        }
    }
    if runtime.paper && runtime.exp001 && snap.launchpad == Launchpad::PonsV2 {
        let started = runtime.exp_started_at;
        let discovered = runtime
            .token_discovered
            .get(&(snap.chain, snap.token_address.clone()))
            .copied()
            .unwrap_or_else(|| {
                snap.snapshot_time - chrono::Duration::milliseconds(snap.age_ms.max(0))
            });
        let eligible = started
            .map(|t| prospective_entry_eligible(discovered, snap.snapshot_time, t))
            .unwrap_or(false);
        if eligible {
            queue_exp001_arms(
                runtime,
                snap,
                history,
                &ctx,
                vec.id,
                sec.as_ref().and_then(|s| s.id),
            );
        }
    } else if runtime.paper
        && !runtime.exp001
        && snap.launchpad == Launchpad::PonsV2
        && !runtime.entered.contains(&key)
    {
        let smoke = smoke_decide(&ctx, &runtime.cfg);
        if smoke.enter {
            tracing::info!(
                token = %snap.token_address,
                age_ms = snap.age_ms,
                "smoke policy queued pending paper entry"
            );
            runtime.entered.insert(key);
            let mut snaps = history.to_vec();
            if !snaps.iter().any(|s| s.age_ms == snap.age_ms) {
                snaps.push(snap.clone());
            }
            runtime.pending.push(PendingPaper {
                chain: snap.chain,
                token: snap.token_address.clone(),
                launchpad: snap.launchpad,
                decision_time: snap.snapshot_time,
                feature_id: vec.id,
                sec_id: sec.as_ref().and_then(|s| s.id),
                quality: snap.data_quality,
                snaps,
                curve: runtime
                    .token_curve
                    .get(&(snap.chain, snap.token_address.clone()))
                    .cloned(),
                attempts: 0,
                experiment_id: None,
                entry_policy: "PIPELINE_SMOKE_POLICY".into(),
                exit_policy: "X1_TIME_2M".into(),
                arm_id: "PIPELINE_SMOKE_POLICY".into(),
                alpha_research_valid: false,
            });
        }
    } else if runtime.paper && snap.launchpad == Launchpad::ClankerV4 {
        let order = crate::prospective::shadow_clanker_order(
            snap.chain,
            &snap.token_address,
            snap.snapshot_time,
            &runtime.cfg.quote_notional,
        );
        if let Some(pg) = pg {
            let _ = pg
                .insert_shadow_order(
                    snap.chain,
                    &snap.token_address,
                    snap.launchpad,
                    snap.snapshot_time,
                    order.side.as_str(),
                    &order.requested_amount,
                    order.status.as_str(),
                    false,
                    "IMPACT_MODEL_PARTIAL_UNISWAP_V4",
                    serde_json::json!({
                        "feature_vector_id": vec.id,
                        "security_assessment_id": sec.as_ref().and_then(|s| s.id),
                        "research_valid_execution": false,
                    }),
                )
                .await;
        }
    }
    if snap.snapshot_kind == crate::state::snapshot::SnapshotKind::Milestone {
        if let Some(pg) = pg {
            let _ = persist_descriptive(pg, snap, history).await;
        }
    }
    Ok(())
}

fn queue_exp001_arms(
    runtime: &mut LiveResearchRuntime,
    snap: &TokenStateSnapshot,
    history: &[TokenStateSnapshot],
    ctx: &StrategyContext<'_>,
    feature_id: Option<i64>,
    sec_id: Option<i64>,
) {
    let Some(exp) = runtime.experiment_id.clone() else {
        return;
    };
    if crate::prospective::in_pons_snipe_window(&runtime.cfg, snap.launchpad, snap.age_ms) {
        return;
    }
    if ctx.candidate != CandidateState::Eligible {
        return;
    }
    let mut snaps = history.to_vec();
    if !snaps.iter().any(|s| s.age_ms == snap.age_ms) {
        snaps.push(snap.clone());
    }
    let curve = runtime
        .token_curve
        .get(&(snap.chain, snap.token_address.clone()))
        .cloned();
    for p in ProspectivePolicy::all() {
        let d = p.decide(ctx);
        if !d.enter {
            continue;
        }
        for exit in EXIT_POLICIES {
            let arm = arm_id_for(&exp, p.id(), exit);
            let key = (snap.chain, snap.token_address.clone(), arm.clone());
            if runtime.entered_arms.contains(&key) {
                continue;
            }
            runtime.entered_arms.insert(key);
            runtime.pending.push(PendingPaper {
                chain: snap.chain,
                token: snap.token_address.clone(),
                launchpad: snap.launchpad,
                decision_time: snap.snapshot_time,
                feature_id,
                sec_id,
                quality: snap.data_quality,
                snaps: snaps.clone(),
                curve: curve.clone(),
                attempts: 0,
                experiment_id: Some(exp.to_string()),
                entry_policy: p.id().into(),
                exit_policy: (*exit).into(),
                arm_id: arm,
                alpha_research_valid: true,
            });
        }
    }
}

fn snap_px(s: &TokenStateSnapshot) -> Option<f64> {
    let t: f64 = s.last_trade_token_raw.as_ref()?.parse().ok()?;
    let q: f64 = s.last_trade_quote_raw.as_ref()?.parse().ok()?;
    if t > 0.0 && q > 0.0 && t.is_finite() && q.is_finite() {
        Some(q / t)
    } else {
        None
    }
}

async fn persist_descriptive(
    pg: &PostgresStore,
    snap: &TokenStateSnapshot,
    history: &[TokenStateSnapshot],
) -> Result<()> {
    let mut series: Vec<(i64, f64)> = history
        .iter()
        .filter(|s| s.chain == snap.chain && s.token_address == snap.token_address)
        .filter_map(|s| snap_px(s).map(|p| (s.age_ms, p)))
        .collect();
    if let Some(p) = snap_px(snap) {
        series.push((snap.age_ms, p));
    }
    let ref_px = series.first().map(|(_, p)| *p);
    let mut o = DescriptiveTokenOutcome::from_prices(
        snap.token_address.clone(),
        snap.snapshot_time,
        ref_px,
        &series,
    );
    o.chain = snap.chain;
    o.launchpad = snap.launchpad;
    o.source = "live_snapshots".into();
    o.capabilities.execution_valid = false;
    o.capabilities.paper_live_valid = snap.launchpad == Launchpad::PonsV2;
    o.maturity = crate::sim::descriptive::OutcomeMaturity::for_live_age(snap.age_ms);
    if matches!(
        o.maturity,
        crate::sim::descriptive::OutcomeMaturity::Pending
    ) {
        DiscoveryMetrics::prospective_outcome_pending();
    }
    let _ = pg.insert_descriptive_outcome(&o).await?;
    Ok(())
}

async fn manage_open_positions(
    runtime: &mut LiveResearchRuntime,
    snap: &TokenStateSnapshot,
    history: &[TokenStateSnapshot],
    pg: Option<&PostgresStore>,
    curve_reader: Option<&PonsCurveReader>,
) {
    if !runtime.paper {
        return;
    }
    let mut book: Vec<TokenStateSnapshot> = history
        .iter()
        .filter(|s| s.chain == snap.chain && s.token_address == snap.token_address)
        .cloned()
        .collect();
    if !book.iter().any(|s| s.age_ms == snap.age_ms) {
        book.push(snap.clone());
    }
    let experiment_id = runtime.experiment_id.clone();
    for pos in &mut runtime.positions {
        if pos.chain != snap.chain || pos.token != snap.token_address {
            continue;
        }
        if pos.status != PositionStatus::Open {
            continue;
        }
        pos.mark(snap, &runtime.cfg.fees);
        let exit_id = parse_exit_policy(&pos.strategy_policy_id);
        let mut eval_snap = snap.clone();
        eval_snap.snapshot_time = Utc::now();
        let Some((reason, amt, full)) = ({
            let policy = exit_policy(exit_id);
            let mgr = PositionManager {
                policy: policy.as_ref(),
                fees: &runtime.cfg.fees,
            };
            mgr.evaluate(pos, &eval_snap, None, None)
        }) else {
            continue;
        };
        tracing::info!(
            token = %pos.token,
            arm = %pos.strategy_policy_id,
            reason = reason.as_str(),
            requested = %amt,
            "paper exit signal"
        );
        let mut cfg = runtime.cfg.clone();
        cfg.delay.rh_fast_ms = 0;
        cfg.delay.rh_base_ms = 0;
        cfg.delay.rh_slow_ms = 0;
        cfg.retry.retry_delay_ms = 0;
        if book.is_empty() {
            book.push(wall_clock_snap(
                pos.chain,
                &pos.token,
                pos.launchpad,
                Utc::now(),
            ));
        }
        let mut curve_state: Option<PonsCurveState> = None;
        let mut curve_state_id: Option<i64> = None;
        let mut curve_fail: Option<String> = None;
        if snap.launchpad == Launchpad::PonsV2 {
            if let (Some(reader), Some(curve)) = (
                curve_reader,
                runtime
                    .token_curve
                    .get(&(snap.chain, snap.token_address.clone()))
                    .cloned(),
            ) {
                match reader.read(&snap.token_address, &curve, None).await {
                    Ok(st) => {
                        annotate_book(&mut book, &st);
                        cfg.fees.pons_curve_bps = st.quote_fee_bps();
                        if let Some(pg) = pg {
                            curve_state_id = pg.insert_pons_curve_state(&st).await.ok();
                        }
                        curve_state = Some(st);
                    }
                    Err(e) => {
                        tracing::warn!(
                            token = %snap.token_address,
                            reason = %e.reason(),
                            "pons curve read failed on paper exit; not faking a sell"
                        );
                        *runtime.fail_reasons.entry(e.kind.as_str()).or_insert(0) += 1;
                        curve_fail = Some(e.reason());
                    }
                }
            }
        }
        let audit = reason.audit_label(exit_id);
        let fill_as_of = eval_snap.snapshot_time;
        let fill = if let Some(reason_s) = curve_fail.clone() {
            crate::sim::exec::ExecutionResult::empty(
                crate::sim::types::OrderSide::Sell,
                fill_as_of,
                fill_as_of,
                "0".into(),
                amt.clone(),
                ExecutionStatus::UnavailableMarketState,
                crate::sim::types::ExecutionQuality::NonResearchValid,
                false,
                reason_s,
                pos.events.len() as u32 + 1,
                runtime.cfg.slippage.adverse_bps,
            )
        } else {
            let emergency = reason.is_emergency();
            let mut fill = crate::prospective::paper_exit(
                &book,
                snap.chain,
                &snap.token_address,
                snap.launchpad,
                fill_as_of,
                &amt,
                &cfg,
                snap.data_quality,
                emergency,
            );
            if let Some(st) = &curve_state {
                fill.curve_state_quality = Some(st.state_quality.as_str().into());
                fill.data_quality = Some(snap.data_quality.as_str().into());
                fill.execution_quality_label =
                    Some(execution_quality_label(st.state_quality, fill.status.is_fill()).into());
            }
            fill
        };
        if !fill.status.is_fill() {
            let cls = classify_paper_failure(fill.reason.as_deref(), fill.status);
            *runtime.fail_reasons.entry(cls).or_insert(0) += 1;
            DiscoveryMetrics::pons_execution_invalid(cls);
        }
        if let Some(pg) = pg {
            let _ = persist_paper_exit(
                pg,
                pos,
                &fill,
                reason,
                audit,
                &amt,
                experiment_id.as_deref(),
                curve_state.as_ref(),
                curve_state_id,
            )
            .await;
        }
        pos.apply_exit(&fill, reason, full);
        if let Some(pg) = pg {
            let _ = pg.update_paper_position(pos).await;
            if let Some(ev) = pos.events.last() {
                let _ = pg
                    .insert_position_event(
                        pos.id,
                        ev.kind.as_str(),
                        ev.at,
                        serde_json::to_value(ev).unwrap_or(serde_json::json!({})),
                    )
                    .await;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn persist_paper_exit(
    pg: &PostgresStore,
    pos: &SimulatedPosition,
    fill: &crate::sim::exec::ExecutionResult,
    reason: crate::sim::types::ExitReason,
    audit: &str,
    requested: &str,
    experiment_id: Option<&str>,
    curve: Option<&PonsCurveState>,
    curve_state_id: Option<i64>,
) -> Result<i64> {
    let attempt_no = pos.events.len() as i32 + 1;
    let payload = serde_json::json!({
        "result": fill,
        "experiment_id": experiment_id,
        "position_id": pos.id,
        "arm_id": pos.strategy_policy_id,
        "exit_reason": reason.as_str(),
        "exit_reason_label": audit,
        "curve": curve,
        "curve_state_id": curve_state_id,
        "getReserves": curve.map(|c| serde_json::json!({
            "virtual_quote_reserve": c.virtual_quote_reserve,
            "virtual_token_reserve": c.virtual_token_reserve,
            "real_quote_reserve": c.real_quote_reserve,
            "real_token_reserve": c.real_token_reserve,
            "feeBps": c.fee_bps,
            "creatorTaxBps": c.creator_tax_bps,
            "block_number": c.block_number,
            "state_quality": c.state_quality.as_str(),
        })),
    });
    let order_id = pg
        .insert_paper_order_ex(
            &pos.strategy_policy_id,
            pos.chain,
            &pos.token,
            "SELL",
            fill.decision_time,
            requested,
            fill.status.as_str(),
            pos.entry_feature_vector_id,
            pos.entry_security_assessment_id,
            payload,
            experiment_id,
            Some(pos.id),
            Some(audit),
        )
        .await?;
    let _ = persist_fill_attempt(
        pg,
        Some(order_id),
        attempt_no,
        fill,
        experiment_id,
        Some(pos.id),
        pos.chain,
        &pos.token,
        "SELL",
        requested,
        curve,
        curve_state_id,
    )
    .await;
    Ok(order_id)
}

#[allow(clippy::too_many_arguments)]
async fn persist_fill_attempt(
    pg: &PostgresStore,
    order_id: Option<i64>,
    attempt_no: i32,
    fill: &crate::sim::exec::ExecutionResult,
    experiment_id: Option<&str>,
    position_id: Option<i64>,
    chain: crate::domain::Chain,
    token: &str,
    side: &str,
    requested: &str,
    curve: Option<&PonsCurveState>,
    curve_state_id: Option<i64>,
) -> Result<i64> {
    let creator = curve.map(|c| c.creator_tax_bps.to_string());
    pg.insert_execution_attempt(
        order_id,
        attempt_no,
        fill.status.as_str(),
        fill.eligible_execution_time,
        fill.actual_simulated_fill_time,
        Some(&fill.filled_quote),
        Some(&fill.filled_token),
        fill.reason.as_deref(),
        serde_json::to_value(fill).unwrap_or(serde_json::json!({})),
        experiment_id,
        position_id,
        Some(chain),
        Some(token),
        Some(side),
        Some(fill.decision_time),
        curve.and_then(|c| c.block_number.map(|b| b as i64)),
        curve.and_then(|c| c.block_hash.as_deref()),
        curve_state_id,
        Some(requested),
        Some(&fill.effective_fill_price_1e18),
        fill.price_impact_bps.map(|v| v as i32),
        Some(fill.slippage_bps as i32),
        Some(&fill.protocol_fee),
        creator.as_deref(),
        Some(&fill.snipe_tax),
        fill.execution_quality_label
            .as_deref()
            .or(Some(fill.quality.as_str())),
        fill.curve_state_quality.as_deref(),
    )
    .await
}

fn wall_clock_snap(
    chain: Chain,
    token: &str,
    launchpad: Launchpad,
    now: chrono::DateTime<Utc>,
) -> TokenStateSnapshot {
    use crate::domain::{CanonicalStatus, Finality, QualityStatus};
    use crate::state::lifecycle::TokenLifecycleState;
    use crate::state::market::MarketState;
    use crate::state::rolling::RollingWindowSnapshot;
    use crate::state::snapshot::{SnapshotKind, WalletSnapshot};
    let empty = |ms: i64| RollingWindowSnapshot {
        duration_ms: ms,
        ..Default::default()
    };
    TokenStateSnapshot {
        id: None,
        chain,
        token_address: token.into(),
        launchpad,
        snapshot_time: now,
        age_ms: 0,
        snapshot_kind: SnapshotKind::Periodic,
        lifecycle_trigger: None,
        lifecycle_state: TokenLifecycleState::CurveActive,
        quote_asset: None,
        buy_count_total: 0,
        sell_count_total: 0,
        unique_buyers_total: 0,
        unique_sellers_total: 0,
        buy_quote_volume_raw_total: "0".into(),
        sell_quote_volume_raw_total: "0".into(),
        buy_token_volume_raw_total: "0".into(),
        sell_token_volume_raw_total: "0".into(),
        creator_buy_count: 0,
        creator_sell_count: 0,
        creator_buy_quote_raw: "0".into(),
        creator_sell_quote_raw: "0".into(),
        last_trade_side: None,
        last_trade_token_raw: None,
        last_trade_quote_raw: None,
        last_trade_token_decimals: None,
        last_trade_quote_decimals: None,
        curve_progress_bps: None,
        graduation_progress_bps: None,
        market_state_type: "UNKNOWN".into(),
        market_state: MarketState::Unknown,
        rolling_5s: empty(5_000),
        rolling_15s: empty(15_000),
        rolling_30s: empty(30_000),
        rolling_60s: empty(60_000),
        rolling_120s: empty(120_000),
        rolling_300s: empty(300_000),
        rolling_900s: empty(900_000),
        as_of_event_id: None,
        as_of_block: None,
        as_of_slot: None,
        as_of_event_order: "wall".into(),
        data_quality: QualityStatus::LiveComplete,
        source_session_id: None,
        canonical_status: CanonicalStatus::Canonical,
        finality: Finality::Confirmed,
        version: 1,
        superseded: false,
        fingerprint: String::new(),
        created_at: now,
        wallet: WalletSnapshot::default(),
    }
}

fn annotate_book(book: &mut [TokenStateSnapshot], state: &PonsCurveState) {
    for s in book.iter_mut() {
        overlay_snapshot(s, state);
    }
}

async fn persist_position(pg: &PostgresStore, pos: &SimulatedPosition) -> Result<i64> {
    pg.insert_open_paper_position(pos).await
}

pub async fn live_tick_once(
    state: &Arc<Mutex<StateEngine>>,
    sched: &Arc<Mutex<LiveMilestoneScheduler>>,
    store: &impl EventStore,
    runtime: &mut LiveResearchRuntime,
    pg: Option<&PostgresStore>,
    curve_reader: Option<&PonsCurveReader>,
) -> Result<usize> {
    let started = Instant::now();
    let (now, snaps) = {
        let mut g = state.lock().expect("state");
        let now = g.clock.now();
        let keys = g.watched_keys();
        {
            let mut s = sched.lock().expect("sched");
            for k in &keys {
                if let Some(d) = g.token_discovered_at(k) {
                    s.register(k, d.unix_ms, &g.schedule);
                }
            }
            let _due = s.pop_due(now.unix_ms);
        }
        for k in &keys {
            if let Some(t) = g.get(k.chain, &k.token) {
                if let Some(c) = &t.curve {
                    runtime
                        .token_curve
                        .insert((k.chain, k.token.clone()), c.clone());
                }
                if let Some(dt) =
                    chrono::DateTime::<Utc>::from_timestamp_millis(t.discovered_at.unix_ms)
                {
                    runtime
                        .token_discovered
                        .insert((k.chain, k.token.clone()), dt);
                }
            }
        }
        let snaps = g.tick_live();
        (now, snaps)
    };
    let n = snaps.len();
    for mut snap in snaps {
        let history: Vec<TokenStateSnapshot> = {
            let g = state.lock().expect("state");
            g.history
                .iter()
                .filter(|s| s.chain == snap.chain && s.token_address == snap.token_address)
                .cloned()
                .collect()
        };
        persist_and_research(store, &mut snap, &history, runtime, pg).await?;
        manage_open_positions(&mut *runtime, &snap, &history, pg, curve_reader).await;
    }
    let due: Vec<(Chain, String)> = runtime
        .positions
        .iter()
        .filter(|p| p.status == PositionStatus::Open)
        .map(|p| (p.chain, p.token.clone()))
        .collect();
    let now_ts = Utc::now();
    for (chain, token) in due {
        let history: Vec<TokenStateSnapshot> = {
            let g = state.lock().expect("state");
            g.history
                .iter()
                .filter(|s| s.chain == chain && s.token_address == token)
                .cloned()
                .collect()
        };
        let launchpad = runtime
            .positions
            .iter()
            .find(|p| p.chain == chain && p.token == token)
            .map(|p| p.launchpad)
            .unwrap_or(Launchpad::PonsV2);
        let mut snap = history
            .last()
            .cloned()
            .unwrap_or_else(|| wall_clock_snap(chain, &token, launchpad, now_ts));
        snap.snapshot_time = now_ts;
        manage_open_positions(runtime, &snap, &history, pg, curve_reader).await;
    }
    flush_pending_paper(state, runtime, pg, curve_reader).await;
    DiscoveryMetrics::live_tick_ms(started.elapsed().as_millis() as i64);
    let _ = now;
    Ok(n)
}

async fn flush_pending_paper(
    state: &Arc<Mutex<StateEngine>>,
    runtime: &mut LiveResearchRuntime,
    pg: Option<&PostgresStore>,
    curve_reader: Option<&PonsCurveReader>,
) {
    if runtime.pending.is_empty() {
        return;
    }
    let now = Utc::now();
    let pending = std::mem::take(&mut runtime.pending);
    let mut keep = Vec::new();
    for mut p in pending {
        let delay = runtime.cfg.delay.delay_ms(p.chain);
        let due = p.decision_time + chrono::Duration::milliseconds(delay);
        if now < due {
            keep.push(p);
            continue;
        }
        let mut book = p.snaps.clone();
        {
            let g = state.lock().expect("state");
            for s in g
                .history
                .iter()
                .filter(|s| s.chain == p.chain && s.token_address == p.token)
            {
                if !book
                    .iter()
                    .any(|b| b.age_ms == s.age_ms && b.snapshot_time == s.snapshot_time)
                {
                    book.push(s.clone());
                }
            }
            if p.curve.is_none() {
                p.curve = g
                    .get(p.chain, &p.token)
                    .and_then(|t| t.curve.clone())
                    .or_else(|| {
                        runtime
                            .token_curve
                            .get(&(p.chain, p.token.clone()))
                            .cloned()
                    });
            }
        }
        if book.is_empty() {
            keep.push(p);
            continue;
        }
        let mut cfg = runtime.cfg.clone();
        cfg.delay.rh_fast_ms = 0;
        cfg.delay.rh_base_ms = 0;
        cfg.delay.rh_slow_ms = 0;
        cfg.retry.max_entry_retries = 0;

        let mut curve_state: Option<PonsCurveState> = None;
        if p.launchpad == Launchpad::PonsV2 {
            match (curve_reader, p.curve.as_deref()) {
                (Some(reader), Some(curve)) => match reader.read(&p.token, curve, None).await {
                    Ok(st) => {
                        if !st.is_tradeable() {
                            p.attempts += 1;
                            let reason = "INVALID_CURVE_STATE";
                            record_failed_order(runtime, pg, &p, now, reason).await;
                            continue;
                        }
                        annotate_book(&mut book, &st);
                        cfg.fees.pons_curve_bps = st.quote_fee_bps();
                        if let Some(pg) = pg {
                            let _ = pg.insert_pons_curve_state(&st).await;
                        }
                        curve_state = Some(st);
                    }
                    Err(e) => {
                        p.attempts += 1;
                        if matches!(
                            e.kind,
                            CurveReadErrorKind::Timeout | CurveReadErrorKind::RateLimit
                        ) && p.attempts < 8
                        {
                            tracing::warn!(
                                token = %p.token,
                                attempt = p.attempts,
                                reason = %e.reason(),
                                "pons curve read limited; retrying pending paper"
                            );
                            keep.push(p);
                            continue;
                        }
                        record_failed_order(runtime, pg, &p, now, &e.reason()).await;
                        continue;
                    }
                },
                (_, None) => {
                    record_failed_order(runtime, pg, &p, now, "CURVE_NOT_FOUND").await;
                    continue;
                }
                (None, Some(_)) => {
                    record_failed_order(
                        runtime,
                        pg,
                        &p,
                        now,
                        "PROVIDER_TIMEOUT: no curve reader configured",
                    )
                    .await;
                    continue;
                }
            }
        }

        let mut fill = crate::prospective::paper_entry_at(
            &book,
            p.chain,
            &p.token,
            p.launchpad,
            now,
            now,
            &cfg,
            p.quality,
        );
        if let Some(st) = &curve_state {
            fill.curve_state_quality = Some(st.state_quality.as_str().into());
            fill.data_quality = Some(p.quality.as_str().into());
            fill.execution_quality_label =
                Some(execution_quality_label(st.state_quality, fill.status.is_fill()).into());
        }
        tracing::info!(
            token = %p.token,
            status = fill.status.as_str(),
            reason = fill.reason.as_deref().unwrap_or(""),
            curve_q = fill.curve_state_quality.as_deref().unwrap_or(""),
            exec_q = fill.execution_quality_label.as_deref().unwrap_or(""),
            snaps = book.len(),
            "paper flush"
        );
        runtime.smoke_orders += 1;
        DiscoveryMetrics::paper_order();
        let fail_class = classify_paper_failure(fill.reason.as_deref(), fill.status);
        let mut buy_order_id: Option<i64> = None;
        if let Some(pg) = pg {
            let payload = serde_json::json!({
                "result": fill,
                "alpha_research_valid": p.alpha_research_valid,
                "execution_model_valid": fill.status.is_fill()
                    && curve_state
                        .as_ref()
                        .map(|s| s.state_quality.research_valid_live_paper())
                        .unwrap_or(false),
                "curve_state_quality": fill.curve_state_quality,
                "execution_quality": fill.execution_quality_label,
                "data_quality": fill.data_quality,
                "experiment_id": p.experiment_id,
                "arm_id": p.arm_id,
                "fail_class": if fill.status.is_fill() { serde_json::Value::Null } else { serde_json::Value::String(fail_class.into()) },
            });
            buy_order_id = pg
                .insert_paper_order_ex(
                    &p.arm_id,
                    p.chain,
                    &p.token,
                    "BUY",
                    p.decision_time,
                    &runtime.cfg.quote_notional,
                    fill.status.as_str(),
                    p.feature_id,
                    p.sec_id,
                    payload,
                    p.experiment_id.as_deref(),
                    None,
                    None,
                )
                .await
                .ok();
            if let Some(oid) = buy_order_id {
                let _ = persist_fill_attempt(
                    pg,
                    Some(oid),
                    p.attempts.max(1) as i32,
                    &fill,
                    p.experiment_id.as_deref(),
                    None,
                    p.chain,
                    &p.token,
                    "BUY",
                    &runtime.cfg.quote_notional,
                    curve_state.as_ref(),
                    None,
                )
                .await;
            }
        }
        match fill.status {
            ExecutionStatus::Filled => {
                runtime.smoke_fills += 1;
                DiscoveryMetrics::paper_fill();
                DiscoveryMetrics::pons_execution_valid_fill();
            }
            ExecutionStatus::PartialFill => {
                runtime.smoke_partial += 1;
                runtime.smoke_fills += 1;
                DiscoveryMetrics::paper_fill();
                DiscoveryMetrics::pons_execution_valid_fill();
            }
            _ => {
                runtime.smoke_failed += 1;
                *runtime.fail_reasons.entry(fail_class).or_insert(0) += 1;
                DiscoveryMetrics::pons_execution_invalid(fail_class);
            }
        }
        if fill.status.is_fill() {
            let mut pos = SimulatedPosition::open(
                runtime.next_pos,
                p.chain,
                p.token.clone(),
                p.launchpad,
                p.arm_id.clone(),
                &fill,
                p.feature_id,
                p.sec_id,
            );
            pos.entry_research_valid = p.alpha_research_valid
                && curve_state
                    .as_ref()
                    .map(|s| s.state_quality.research_valid_live_paper())
                    .unwrap_or(false);
            runtime.next_pos += 1;
            if let Some(pg) = pg {
                if let Ok(id) = persist_position(pg, &pos).await {
                    pos.id = id;
                    DiscoveryMetrics::paper_position(p.chain, p.launchpad);
                    if let Some(ev) = pos.events.first() {
                        let _ = pg
                            .insert_position_event(
                                id,
                                ev.kind.as_str(),
                                ev.at,
                                serde_json::to_value(ev).unwrap_or(serde_json::json!({})),
                            )
                            .await;
                    }
                    if let Some(oid) = buy_order_id {
                        let _ = pg.attach_order_position(oid, id).await;
                    }
                }
            }
            runtime.positions.push(pos);
        }
    }
    runtime.pending = keep;
}

async fn record_failed_order(
    runtime: &mut LiveResearchRuntime,
    pg: Option<&PostgresStore>,
    p: &PendingPaper,
    now: chrono::DateTime<Utc>,
    reason: &str,
) {
    runtime.smoke_orders += 1;
    runtime.smoke_failed += 1;
    DiscoveryMetrics::paper_order();
    let cls = classify_paper_failure(Some(reason), ExecutionStatus::UnavailableMarketState);
    *runtime.fail_reasons.entry(cls).or_insert(0) += 1;
    DiscoveryMetrics::pons_execution_invalid(cls);
    let fill = crate::sim::exec::ExecutionResult::empty(
        crate::sim::types::OrderSide::Buy,
        p.decision_time,
        now,
        runtime.cfg.quote_notional.clone(),
        "0".into(),
        ExecutionStatus::UnavailableMarketState,
        crate::sim::types::ExecutionQuality::NonResearchValid,
        false,
        reason,
        p.attempts,
        runtime.cfg.slippage.adverse_bps,
    );
    tracing::info!(
        token = %p.token,
        reason,
        class = cls,
        "smoke paper order failed before fill math"
    );
    if let Some(pg) = pg {
        let payload = serde_json::json!({
            "result": fill,
            "alpha_research_valid": p.alpha_research_valid,
            "execution_model_valid": false,
            "experiment_id": p.experiment_id,
            "arm_id": p.arm_id,
            "fail_class": cls,
        });
        let _ = pg
            .insert_paper_order(
                &p.arm_id,
                p.chain,
                &p.token,
                "BUY",
                p.decision_time,
                &runtime.cfg.quote_notional,
                fill.status.as_str(),
                p.feature_id,
                p.sec_id,
                payload,
            )
            .await;
    }
}

pub async fn restore_open_positions(
    pg: &PostgresStore,
    runtime: &mut LiveResearchRuntime,
) -> Result<usize> {
    restore_open_positions_prefixed(pg, runtime, None).await
}

pub async fn restore_open_positions_prefixed(
    pg: &PostgresStore,
    runtime: &mut LiveResearchRuntime,
    prefix: Option<&str>,
) -> Result<usize> {
    let rows = pg.load_open_paper_positions_prefixed(prefix).await?;
    let n = rows.len();
    for mut p in rows {
        if p.status == PositionStatus::SessionEndedOpen {
            p.status = PositionStatus::Open;
        }
        runtime.next_pos = runtime.next_pos.max(p.id.saturating_add(1));
        DiscoveryMetrics::paper_position_recovered(p.chain, p.launchpad);
        runtime.entered.insert((p.chain, p.token.clone()));
        runtime
            .entered_arms
            .insert((p.chain, p.token.clone(), p.strategy_policy_id.clone()));
        runtime.recovered += 1;
        if let Err(e) = pg.update_paper_position(&p).await {
            tracing::warn!(error = %e, "failed to reopen restored paper position");
        }
        runtime.positions.push(p);
    }
    Ok(n)
}

pub async fn hydrate_watched_tokens(
    store: &PostgresStore,
    state: &Arc<Mutex<StateEngine>>,
) -> Result<usize> {
    let recent = store
        .list_recent_discovered(chrono::Duration::hours(2))
        .await?;
    let mut n = 0usize;
    for (chain, token) in recent {
        let key = TokenKey::new(chain, &token);
        let mut events = Vec::new();
        if let Some(d) = store.load_token_discovered(chain, &token).await? {
            events.push(CanonicalEvent::TokenDiscovered(Box::new(d)));
        }
        for t in store.load_token_trades(chain, &token).await? {
            events.push(CanonicalEvent::Trade(Box::new(t)));
        }
        for l in store.load_token_lifecycle(chain, &token).await? {
            events.push(CanonicalEvent::Lifecycle(Box::new(l)));
        }
        if events.is_empty() {
            continue;
        }
        let existing = store
            .list_snapshots(chain, &token, false)
            .await
            .unwrap_or_default();
        {
            let mut g = state.lock().expect("state");
            let _ = g.rebuild_token(key.clone(), events);
            g.discard_snapshot_buffer();
            g.mark_already_emitted(&key, &existing);
        }
        n += 1;
    }
    Ok(n)
}

pub fn end_session(runtime: &mut LiveResearchRuntime) {
    let at = Utc::now();
    for p in &mut runtime.positions {
        if p.status == PositionStatus::Open {
            p.end_session_open(at);
        }
    }
}

impl LiveResearchRuntime {
    pub fn skip_duplicate(&self, chain: Chain, token: &str) -> bool {
        self.entered.contains(&(chain, token.to_string()))
    }
}
