//! ExecutionEngine trait + historical/paper implementations. No broadcast. No keys.
#![allow(clippy::too_many_arguments)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{Chain, Launchpad, QualityStatus};
use crate::error::Result;
use crate::state::TokenStateSnapshot;

use super::impact::executable_fill;
use super::models::SimConfig;
use super::types::{
    ExecutionQuality, ExecutionStatus, ExitReason, OrderSide, EXECUTION_MODEL_VERSION,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryRequest {
    pub chain: Chain,
    pub token: String,
    pub launchpad: Launchpad,
    pub decision_time: DateTime<Utc>,
    pub feature_vector_id: Option<i64>,
    pub candidate_transition_id: Option<i64>,
    pub security_assessment_id: Option<i64>,
    pub side: OrderSide,
    pub quote_notional: String,
    pub max_slippage_bps: u32,
    pub strategy_policy_id: String,
    pub simulation_run_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitRequest {
    pub position_id: i64,
    pub chain: Chain,
    pub token: String,
    pub launchpad: Launchpad,
    pub decision_time: DateTime<Utc>,
    pub token_amount_requested: String,
    pub reason: ExitReason,
    pub max_slippage_bps: u32,
    pub simulation_run_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionQuote {
    pub status: ExecutionStatus,
    pub quality: ExecutionQuality,
    pub reference_price_1e18: String,
    pub expected_token: String,
    pub expected_quote: String,
    pub price_impact_bps: Option<u32>,
    pub protocol_fee: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub id: Option<i64>,
    pub attempt_number: u32,
    pub status: ExecutionStatus,
    pub quality: ExecutionQuality,
    pub research_valid: bool,
    pub side: OrderSide,
    pub requested_quote: String,
    pub requested_token: String,
    pub filled_quote: String,
    pub filled_token: String,
    pub fill_fraction_bps: u32,
    pub decision_time: DateTime<Utc>,
    pub eligible_execution_time: DateTime<Utc>,
    pub actual_simulated_fill_time: Option<DateTime<Utc>>,
    pub reference_price_1e18: String,
    pub effective_fill_price_1e18: String,
    pub price_impact_bps: Option<u32>,
    pub slippage_bps: u32,
    pub protocol_fee: String,
    pub snipe_tax: String,
    pub network_fee: String,
    pub priority_fee: String,
    pub tip: String,
    pub total_cost: String,
    pub snapshot_id: Option<i64>,
    pub reason: Option<String>,
    pub execution_model_version: String,
    #[serde(default)]
    pub curve_state_quality: Option<String>,
    #[serde(default)]
    pub data_quality: Option<String>,
    #[serde(default)]
    pub execution_quality_label: Option<String>,
}

impl ExecutionResult {
    pub fn empty(
        side: OrderSide,
        decision: DateTime<Utc>,
        eligible: DateTime<Utc>,
        req_q: String,
        req_t: String,
        status: ExecutionStatus,
        quality: ExecutionQuality,
        research_valid: bool,
        reason: impl Into<String>,
        attempt: u32,
        slip: u32,
    ) -> Self {
        Self {
            id: None,
            attempt_number: attempt,
            status,
            quality,
            research_valid: research_valid && quality.research_valid() && status.is_fill(),
            side,
            requested_quote: req_q,
            requested_token: req_t,
            filled_quote: "0".into(),
            filled_token: "0".into(),
            fill_fraction_bps: 0,
            decision_time: decision,
            eligible_execution_time: eligible,
            actual_simulated_fill_time: None,
            reference_price_1e18: "0".into(),
            effective_fill_price_1e18: "0".into(),
            price_impact_bps: None,
            slippage_bps: slip,
            protocol_fee: "0".into(),
            snipe_tax: "0".into(),
            network_fee: "0".into(),
            priority_fee: "0".into(),
            tip: "0".into(),
            total_cost: "0".into(),
            snapshot_id: None,
            reason: Some(reason.into()),
            execution_model_version: EXECUTION_MODEL_VERSION.into(),
            curve_state_quality: None,
            data_quality: None,
            execution_quality_label: None,
        }
    }
}

#[async_trait]
pub trait ExecutionEngine: Send + Sync {
    async fn quote_entry(&self, request: &EntryRequest) -> Result<ExecutionQuote>;
    async fn execute_entry(&self, request: &EntryRequest) -> Result<ExecutionResult>;
    async fn quote_exit(&self, request: &ExitRequest) -> Result<ExecutionQuote>;
    async fn execute_exit(&self, request: &ExitRequest) -> Result<ExecutionResult>;
}

/// Shared fill math. Historical and paper differ only in which snapshot they may see.
pub struct SnapshotBook<'a> {
    pub snapshots: &'a [TokenStateSnapshot],
    /// Latest time the engine is allowed to observe. Paper: clock.now(). Historical: fill time.
    pub as_of: DateTime<Utc>,
}

impl SnapshotBook<'_> {
    pub fn at(
        &self,
        chain: Chain,
        token: &str,
        time: DateTime<Utc>,
    ) -> Option<&TokenStateSnapshot> {
        let cap = if time <= self.as_of { time } else { self.as_of };
        self.snapshots
            .iter()
            .filter(|s| s.chain == chain && s.token_address == token && s.snapshot_time <= cap)
            .max_by_key(|s| s.snapshot_time)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn simulate_side(
    book: &SnapshotBook<'_>,
    chain: Chain,
    token: &str,
    launchpad: Launchpad,
    side: OrderSide,
    decision: DateTime<Utc>,
    amount: &str,
    is_quote: bool,
    cfg: &SimConfig,
    is_entry: bool,
    emergency: bool,
    data_quality: QualityStatus,
) -> ExecutionResult {
    let delay = if emergency {
        (cfg.delay.delay_ms(chain) / 4).max(50)
    } else {
        cfg.delay.delay_ms(chain)
    };
    let max_attempts = cfg.retry.max_attempts(is_entry, emergency);
    let mut last = ExecutionResult::empty(
        side,
        decision,
        decision + chrono::Duration::milliseconds(delay),
        if is_quote { amount.into() } else { "0".into() },
        if is_quote { "0".into() } else { amount.into() },
        ExecutionStatus::NoFill,
        ExecutionQuality::NonResearchValid,
        false,
        "NO_ATTEMPT",
        0,
        cfg.slippage.adverse_bps,
    );
    let research_src = data_quality.is_research_complete();
    let force_snipe = cfg.allow_snipe_window;
    let _ = launchpad;

    for attempt in 0..max_attempts {
        let extra = i64::from(attempt) * cfg.retry.retry_delay_ms;
        let fill_time = decision + chrono::Duration::milliseconds(delay + extra);
        last.eligible_execution_time = fill_time;
        last.attempt_number = attempt + 1;

        if fill_time > book.as_of {
            last.status = ExecutionStatus::NoFill;
            last.reason = Some("FILL_TIME_NOT_YET_AVAILABLE".into());
            last.quality = ExecutionQuality::Modelled;
            break;
        }

        if cfg
            .failure
            .fails(is_entry, token, fill_time.timestamp_millis(), attempt)
        {
            last.status = ExecutionStatus::Failed;
            last.reason = Some("SEEDED_FAILURE".into());
            last.quality = ExecutionQuality::Modelled;
            last.actual_simulated_fill_time = Some(fill_time);
            continue;
        }

        let Some(snap) = book.at(chain, token, fill_time) else {
            last.status = ExecutionStatus::UnavailableMarketState;
            last.reason = Some("NO_SNAPSHOT_AT_FILL_TIME".into());
            last.quality = ExecutionQuality::NonResearchValid;
            continue;
        };

        if !snap.data_quality.is_research_complete() && research_src {
            // snapshot quality can still be historical
        }
        let q_ok = snap.data_quality.is_research_complete();

        let fill = executable_fill(
            snap,
            side,
            amount,
            &cfg.fees,
            &cfg.slippage,
            cfg.max_slippage_bps,
            force_snipe,
        );
        last.snapshot_id = snap.id;
        last.actual_simulated_fill_time = Some(fill_time);
        last.status = fill.status;
        last.quality = fill.quality;
        last.filled_quote = fill.quote_amount.clone();
        last.filled_token = fill.token_amount.clone();
        last.fill_fraction_bps = fill.fill_fraction_bps;
        last.reference_price_1e18 = fill.reference_price_1e18;
        last.effective_fill_price_1e18 = fill.effective_price_1e18;
        last.price_impact_bps = fill.price_impact_bps;
        last.protocol_fee = fill.protocol_fee.clone();
        last.snipe_tax = fill.snipe_tax.clone();
        last.network_fee = cfg.fees.network_fee_quote.clone();
        last.priority_fee = cfg.fees.priority_fee_quote.clone();
        last.tip = cfg.fees.tip_quote.clone();
        last.total_cost = crate::state::amt::add_raw(
            &crate::state::amt::add_raw(&fill.protocol_fee, &fill.snipe_tax),
            &cfg.fees.network_fee_quote,
        );
        last.reason = fill.reason;
        last.research_valid = q_ok && fill.quality.research_valid() && fill.status.is_fill();
        if is_quote {
            last.requested_quote = amount.into();
            last.requested_token = "0".into();
        } else {
            last.requested_token = amount.into();
            last.requested_quote = "0".into();
        }
        if fill.status.is_fill() {
            crate::metrics::DiscoveryMetrics::sim_fill(chain, launchpad, fill.status.as_str());
            return last;
        }
    }
    last.research_valid = false;
    crate::metrics::DiscoveryMetrics::sim_fill(chain, launchpad, last.status.as_str());
    last
}

pub struct HistoricalExecutionEngine<'a> {
    pub book: SnapshotBook<'a>,
    pub cfg: &'a SimConfig,
    pub data_quality: QualityStatus,
}

pub struct PaperExecutionEngine<'a> {
    pub book: SnapshotBook<'a>,
    pub cfg: &'a SimConfig,
    pub data_quality: QualityStatus,
}

/// Intentionally unimplemented. Phase 6 must not broadcast.
pub struct LiveExecutionEngine;

#[async_trait]
impl ExecutionEngine for HistoricalExecutionEngine<'_> {
    async fn quote_entry(&self, request: &EntryRequest) -> Result<ExecutionQuote> {
        Ok(quote_from(
            &self.book,
            request.chain,
            &request.token,
            request.side,
            request.decision_time,
            &request.quote_notional,
            true,
            self.cfg,
        ))
    }

    async fn execute_entry(&self, request: &EntryRequest) -> Result<ExecutionResult> {
        if request.side != OrderSide::Buy {
            return Ok(ExecutionResult::empty(
                request.side,
                request.decision_time,
                request.decision_time,
                request.quote_notional.clone(),
                "0".into(),
                ExecutionStatus::NoFill,
                ExecutionQuality::Modelled,
                false,
                "NO_SHORTING",
                0,
                self.cfg.slippage.adverse_bps,
            ));
        }
        Ok(simulate_side(
            &self.book,
            request.chain,
            &request.token,
            request.launchpad,
            OrderSide::Buy,
            request.decision_time,
            &request.quote_notional,
            true,
            self.cfg,
            true,
            false,
            self.data_quality,
        ))
    }

    async fn quote_exit(&self, request: &ExitRequest) -> Result<ExecutionQuote> {
        Ok(quote_from(
            &self.book,
            request.chain,
            &request.token,
            OrderSide::Sell,
            request.decision_time,
            &request.token_amount_requested,
            false,
            self.cfg,
        ))
    }

    async fn execute_exit(&self, request: &ExitRequest) -> Result<ExecutionResult> {
        Ok(simulate_side(
            &self.book,
            request.chain,
            &request.token,
            request.launchpad,
            OrderSide::Sell,
            request.decision_time,
            &request.token_amount_requested,
            false,
            self.cfg,
            false,
            request.reason.is_emergency(),
            self.data_quality,
        ))
    }
}

#[async_trait]
impl ExecutionEngine for PaperExecutionEngine<'_> {
    async fn quote_entry(&self, request: &EntryRequest) -> Result<ExecutionQuote> {
        HistoricalExecutionEngine {
            book: SnapshotBook {
                snapshots: self.book.snapshots,
                as_of: self.book.as_of,
            },
            cfg: self.cfg,
            data_quality: self.data_quality,
        }
        .quote_entry(request)
        .await
    }

    async fn execute_entry(&self, request: &EntryRequest) -> Result<ExecutionResult> {
        HistoricalExecutionEngine {
            book: SnapshotBook {
                snapshots: self.book.snapshots,
                as_of: self.book.as_of,
            },
            cfg: self.cfg,
            data_quality: self.data_quality,
        }
        .execute_entry(request)
        .await
    }

    async fn quote_exit(&self, request: &ExitRequest) -> Result<ExecutionQuote> {
        HistoricalExecutionEngine {
            book: SnapshotBook {
                snapshots: self.book.snapshots,
                as_of: self.book.as_of,
            },
            cfg: self.cfg,
            data_quality: self.data_quality,
        }
        .quote_exit(request)
        .await
    }

    async fn execute_exit(&self, request: &ExitRequest) -> Result<ExecutionResult> {
        HistoricalExecutionEngine {
            book: SnapshotBook {
                snapshots: self.book.snapshots,
                as_of: self.book.as_of,
            },
            cfg: self.cfg,
            data_quality: self.data_quality,
        }
        .execute_exit(request)
        .await
    }
}

impl LiveExecutionEngine {
    pub fn not_implemented() -> &'static str {
        "LiveExecutionEngine is not implemented in Phase 6. No private keys, no broadcast."
    }
}

#[allow(clippy::too_many_arguments)]
fn quote_from(
    book: &SnapshotBook<'_>,
    chain: Chain,
    token: &str,
    side: OrderSide,
    decision: DateTime<Utc>,
    amount: &str,
    is_quote: bool,
    cfg: &SimConfig,
) -> ExecutionQuote {
    let fill_time = decision + chrono::Duration::milliseconds(cfg.delay.delay_ms(chain));
    let Some(snap) = book.at(chain, token, fill_time.min(book.as_of)) else {
        return ExecutionQuote {
            status: ExecutionStatus::UnavailableMarketState,
            quality: ExecutionQuality::NonResearchValid,
            reference_price_1e18: "0".into(),
            expected_token: "0".into(),
            expected_quote: "0".into(),
            price_impact_bps: None,
            protocol_fee: "0".into(),
            reason: Some("NO_SNAPSHOT".into()),
        };
    };
    let amt_side = if is_quote { OrderSide::Buy } else { side };
    let fill = executable_fill(
        snap,
        amt_side,
        amount,
        &cfg.fees,
        &cfg.slippage,
        cfg.max_slippage_bps,
        cfg.allow_snipe_window,
    );
    ExecutionQuote {
        status: fill.status,
        quality: fill.quality,
        reference_price_1e18: fill.reference_price_1e18,
        expected_token: fill.token_amount,
        expected_quote: fill.quote_amount,
        price_impact_bps: fill.price_impact_bps,
        protocol_fee: fill.protocol_fee,
        reason: fill.reason,
    }
}
