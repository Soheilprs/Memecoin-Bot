use std::time::Instant;

use chrono::{DateTime, Utc};
use metrics::{counter, gauge, histogram};

use crate::domain::{
    Chain, Launchpad, LifecycleObserved, RawEvent, TokenDiscovered, TradeObserved,
};

#[derive(Debug, Clone, Default)]
pub struct DiscoveryMetrics;

impl DiscoveryMetrics {
    pub fn raw_received(&self, raw: &RawEvent) {
        counter!(
            "raw_events_received_total",
            "chain" => raw.chain().as_str()
        )
        .increment(1);
    }

    pub fn live_event(&self, raw: &RawEvent) {
        counter!(
            "live_events_total",
            "chain" => raw.chain().as_str(),
            "source" => source_label(&raw.source)
        )
        .increment(1);
    }

    pub fn backfill_event(&self, raw: &RawEvent) {
        counter!(
            "backfill_events_total",
            "chain" => raw.chain().as_str(),
            "source" => source_label(&raw.source)
        )
        .increment(1);
    }

    pub fn decode_success(&self, token: &TokenDiscovered) {
        counter!(
            "token_discovered_total",
            "chain" => token.chain.as_str(),
            "launchpad" => token.launchpad.as_str()
        )
        .increment(1);
        counter!(
            "decode_success_total",
            "chain" => token.chain.as_str(),
            "launchpad" => token.launchpad.as_str()
        )
        .increment(1);
    }

    pub fn trade(&self, trade: &TradeObserved) {
        counter!(
            "trade_events_total",
            "chain" => trade.chain.as_str(),
            "launchpad" => trade.launchpad.as_str(),
            "event_type" => trade.side.as_str()
        )
        .increment(1);
    }

    pub fn lifecycle(&self, life: &LifecycleObserved) {
        counter!(
            "lifecycle_events_total",
            "chain" => life.chain.as_str(),
            "launchpad" => life.launchpad.as_str(),
            "event_type" => life.lifecycle_type.as_str()
        )
        .increment(1);
    }

    pub fn decode_failure(&self, raw: &RawEvent) {
        counter!(
            "decode_failure_total",
            "chain" => raw.chain().as_str()
        )
        .increment(1);
    }

    pub fn unknown(&self, raw: &RawEvent) {
        counter!(
            "unknown_event_total",
            "chain" => raw.chain().as_str()
        )
        .increment(1);
    }

    pub fn duplicate(&self, raw: &RawEvent) {
        counter!(
            "duplicate_event_total",
            "chain" => raw.chain().as_str()
        )
        .increment(1);
    }

    pub fn orphaned(&self, raw: &RawEvent) {
        self.orphaned_id(raw.chain());
    }

    pub fn orphaned_id(&self, chain: Chain) {
        counter!(
            "orphaned_event_total",
            "chain" => chain.as_str()
        )
        .increment(1);
    }

    pub fn reconnect(&self, chain: Chain) {
        counter!(
            "reconnect_total",
            "chain" => chain.as_str()
        )
        .increment(1);
    }

    pub fn backfilled(&self, chain: Chain) {
        counter!(
            "backfilled_event_total",
            "chain" => chain.as_str()
        )
        .increment(1);
    }

    pub fn stream_gap(&self, chain: Chain) {
        counter!(
            "stream_gap_total",
            "chain" => chain.as_str()
        )
        .increment(1);
    }

    pub fn stream_gap_recovered(&self, chain: Chain) {
        counter!(
            "stream_gap_recovered_total",
            "chain" => chain.as_str()
        )
        .increment(1);
    }

    pub fn channel_saturated(&self, chain: Chain) {
        counter!(
            "channel_saturated_total",
            "chain" => chain.as_str()
        )
        .increment(1);
    }

    pub fn solana_slots(head: u64, received: u64, persisted: u64, finalized: u64, lag: u64) {
        gauge!("solana_head_slot").set(head as f64);
        gauge!("solana_received_slot").set(received as f64);
        gauge!("solana_persisted_slot").set(persisted as f64);
        gauge!("solana_finalized_slot").set(finalized as f64);
        gauge!("solana_slot_lag").set(lag as f64);
        gauge!("checkpoint_lag_slots", "chain" => "solana").set(lag as f64);
    }

    pub fn solana_missing_slot_range() {
        counter!("solana_missing_slot_ranges_total").increment(1);
    }

    pub fn solana_repair_attempt() {
        counter!("solana_repair_attempt_total").increment(1);
    }

    pub fn solana_repair_success() {
        counter!("solana_repair_success_total").increment(1);
    }

    pub fn ingest_lag_sample(&self, chain: Chain, lag_ms: i64) {
        histogram!("ingest_lag_ms", "chain" => chain.as_str()).record(lag_ms.max(0) as f64);
    }

    pub fn checkpoint_lag_blocks(&self, chain: Chain, lag: i64) {
        gauge!("checkpoint_lag_blocks", "chain" => chain.as_str()).set(lag as f64);
    }

    pub fn checkpoint_lag_slots(&self, chain: Chain, lag: i64) {
        gauge!("checkpoint_lag_slots", "chain" => chain.as_str()).set(lag as f64);
    }

    pub fn chain_head_lag_ms(&self, chain: Chain, lag: i64) {
        gauge!("chain_head_lag_ms", "chain" => chain.as_str()).set(lag as f64);
    }

    pub fn db_write_failure(&self) {
        counter!("db_write_failure_total").increment(1);
    }

    pub fn collection_session(chain: Chain, mode: &str, incomplete: bool) {
        counter!(
            "collection_session_total",
            "chain" => chain.as_str(),
            "mode" => mode_label(mode)
        )
        .increment(1);
        if incomplete {
            counter!(
                "collection_session_incomplete_total",
                "chain" => chain.as_str()
            )
            .increment(1);
        }
    }

    pub fn historical_replay_event() {
        counter!("historical_replay_events_total").increment(1);
    }

    pub fn historical_replay_duration(duration: std::time::Duration) {
        histogram!("historical_replay_duration_seconds").record(duration.as_secs_f64());
    }

    pub fn state_event_processed(chain: Chain) {
        counter!("state_events_processed_total", "chain" => chain.as_str()).increment(1);
    }

    pub fn token_states_active(n: usize) {
        gauge!("token_states_active").set(n as f64);
    }

    pub fn token_state_evicted(chain: Chain) {
        counter!("token_states_evicted_total", "chain" => chain.as_str()).increment(1);
    }

    pub fn snapshot_created(chain: Chain, launchpad: Launchpad, kind: &'static str) {
        counter!(
            "snapshots_created_total",
            "chain" => chain.as_str(),
            "launchpad" => launchpad.as_str(),
            "snapshot_kind" => kind
        )
        .increment(1);
    }

    pub fn snapshots_persisted(n: u64) {
        counter!("snapshots_persisted_total").increment(n);
    }

    pub fn snapshot_persist_lag_ms(ms: i64) {
        histogram!("snapshot_persist_lag_ms").record(ms.max(0) as f64);
    }

    pub fn state_rebuild(chain: Chain) {
        counter!("state_rebuild_total", "chain" => chain.as_str()).increment(1);
    }

    pub fn state_rebuild_failure(chain: Chain) {
        counter!("state_rebuild_failure_total", "chain" => chain.as_str()).increment(1);
    }

    pub fn late_event(chain: Chain) {
        counter!("late_event_total", "chain" => chain.as_str()).increment(1);
    }

    pub fn rolling_window_update(chain: Chain) {
        counter!("rolling_window_update_total", "chain" => chain.as_str()).increment(1);
    }

    pub fn snapshot_queue_depth(n: usize) {
        gauge!("snapshot_queue_depth").set(n as f64);
    }

    pub fn snapshot_queue_saturated() {
        counter!("snapshot_queue_saturated_total").increment(1);
    }

    pub fn security_assessment(chain: Chain, launchpad: Launchpad, verdict: &'static str) {
        counter!(
            "security_assessment_total",
            "chain" => chain.as_str(),
            "launchpad" => launchpad.as_str(),
            "verdict" => verdict
        )
        .increment(1);
        match verdict {
            "PASS" => counter!("security_pass_total").increment(1),
            "WARN" => counter!("security_warn_total").increment(1),
            "REJECT" => {
                counter!("security_reject_total").increment(1);
                counter!("security_hard_reject_total").increment(1);
            }
            _ => counter!("security_unknown_total").increment(1),
        }
    }

    pub fn security_static_latency_ms(ms: i64) {
        histogram!("security_static_latency_ms").record(ms.max(0) as f64);
    }

    pub fn security_queue_depth_inc() {
        counter!("security_queue_enqueued_total").increment(1);
    }

    pub fn security_queue_depth_dec() {
        counter!("security_queue_dequeued_total").increment(1);
    }

    pub fn security_queue_saturated() {
        counter!("security_queue_saturation_total").increment(1);
    }

    pub fn feature_vector(chain: Chain, launchpad: Launchpad) {
        counter!(
            "feature_vector_total",
            "chain" => chain.as_str(),
            "launchpad" => launchpad.as_str()
        )
        .increment(1);
    }

    pub fn feature_compute_latency_ms(ms: i64) {
        histogram!("feature_compute_latency_ms").record(ms.max(0) as f64);
    }

    pub fn feature_queue_depth(n: usize) {
        gauge!("feature_queue_depth").set(n as f64);
    }

    pub fn feature_queue_saturated() {
        counter!("feature_queue_saturation_total").increment(1);
    }

    pub fn candidate_transition(chain: Chain, launchpad: Launchpad, state: &'static str) {
        counter!(
            "candidate_transition_total",
            "chain" => chain.as_str(),
            "launchpad" => launchpad.as_str(),
            "candidate_state" => state
        )
        .increment(1);
    }

    pub fn sim_fill(chain: Chain, launchpad: Launchpad, status: &'static str) {
        counter!(
            "simulation_orders_total",
            "chain" => chain.as_str(),
            "launchpad" => launchpad.as_str(),
            "status" => status
        )
        .increment(1);
        match status {
            "FILLED" => counter!("simulation_fills_total").increment(1),
            "PARTIAL_FILL" => counter!("simulation_partial_fills_total").increment(1),
            "FAILED" | "NO_FILL" | "REJECTED_LIQUIDITY" | "UNAVAILABLE_MARKET_STATE" => {
                counter!("simulation_failed_fills_total").increment(1);
            }
            _ => {}
        }
    }

    pub fn sim_position_open() {
        counter!("simulation_positions_open").increment(1);
    }

    pub fn sim_position_closed() {
        counter!("simulation_positions_closed_total").increment(1);
    }

    pub fn sim_missed_winner() {
        counter!("simulation_missed_winner_total").increment(1);
    }

    pub fn strategy_signal(id: &'static str) {
        counter!("strategy_signals_total", "strategy" => id).increment(1);
    }

    pub fn strategy_entry(id: &'static str) {
        counter!("strategy_entries_total", "strategy" => id).increment(1);
    }

    pub fn experiment_run() {
        counter!("experiment_run_total").increment(1);
    }

    pub fn moonshot(kind: &'static str) {
        counter!("moonshot_total", "kind" => kind).increment(1);
    }

    pub fn sim_unsellable() {
        counter!("simulation_unsellable_total").increment(1);
    }

    pub fn research_token(chain: Chain, launchpad: Launchpad, quality: &'static str) {
        counter!(
            "research_tokens_observed_total",
            "chain" => chain.as_str(),
            "launchpad" => launchpad.as_str(),
            "quality" => quality
        )
        .increment(1);
    }

    pub fn descriptive_outcome(chain: Chain, launchpad: Launchpad, quality: &'static str) {
        counter!(
            "descriptive_outcomes_total",
            "chain" => chain.as_str(),
            "launchpad" => launchpad.as_str(),
            "quality" => quality
        )
        .increment(1);
    }

    pub fn prospective_session(chain: Chain, launchpad: Launchpad) {
        counter!(
            "prospective_sessions_total",
            "chain" => chain.as_str(),
            "launchpad" => launchpad.as_str()
        )
        .increment(1);
    }

    pub fn paper_position(chain: Chain, launchpad: Launchpad) {
        counter!(
            "paper_positions_total",
            "chain" => chain.as_str(),
            "launchpad" => launchpad.as_str()
        )
        .increment(1);
    }

    pub fn paper_position_recovered(chain: Chain, launchpad: Launchpad) {
        counter!(
            "paper_positions_recovered_total",
            "chain" => chain.as_str(),
            "launchpad" => launchpad.as_str()
        )
        .increment(1);
    }

    pub fn cross_chain_wallet() {
        counter!("cross_chain_wallets_seen_total").increment(1);
        counter!("wallet_identity_upserts_total").increment(1);
    }

    pub fn live_feature_vector(chain: Chain, launchpad: Launchpad) {
        counter!(
            "live_feature_vectors_total",
            "chain" => chain.as_str(),
            "launchpad" => launchpad.as_str()
        )
        .increment(1);
    }

    pub fn live_milestone_due() {
        counter!("live_feature_milestone_due_total").increment(1);
    }

    pub fn live_milestone_missed() {
        counter!("live_feature_milestone_missed_total").increment(1);
    }

    pub fn live_milestone_lateness_ms(ms: i64) {
        histogram!("live_feature_milestone_lateness_ms").record(ms.max(0) as f64);
    }

    pub fn live_tick_ms(ms: i64) {
        histogram!("live_tick_ms").record(ms.max(0) as f64);
    }

    pub fn paper_signal(policy: &'static str) {
        counter!("paper_signals_total", "policy" => policy).increment(1);
    }

    pub fn paper_order() {
        counter!("paper_orders_total").increment(1);
    }

    pub fn paper_fill() {
        counter!("paper_fills_total").increment(1);
    }

    pub fn pons_curve_state_read() {
        counter!("pons_curve_state_read_total").increment(1);
    }

    pub fn pons_curve_state_read_failure(reason: &'static str) {
        counter!(
            "pons_curve_state_read_failure_total",
            "reason" => reason
        )
        .increment(1);
    }

    pub fn pons_curve_state_cache_hit() {
        counter!("pons_curve_state_cache_hit_total").increment(1);
    }

    pub fn pons_curve_state_latency_ms(ms: i64) {
        histogram!("pons_curve_state_read_ms").record(ms.max(0) as f64);
    }

    pub fn pons_multicall_batch() {
        counter!("pons_multicall_batch_total").increment(1);
    }

    pub fn pons_execution_valid_fill() {
        counter!("pons_execution_valid_fill_total").increment(1);
    }

    pub fn prospective_signal() {
        counter!("prospective_signal_total").increment(1);
    }
    pub fn prospective_fill() {
        counter!("prospective_fill_total").increment(1);
    }
    pub fn prospective_position() {
        counter!("prospective_position_total").increment(1);
    }
    pub fn prospective_position_recovered() {
        counter!("prospective_position_recovered_total").increment(1);
    }
    pub fn prospective_gap() {
        counter!("prospective_gap_total").increment(1);
    }
    pub fn prospective_pause() {
        counter!("prospective_operational_pause_total").increment(1);
    }

    pub fn pons_execution_invalid(reason: &'static str) {
        counter!(
            "pons_execution_invalid_total",
            "reason" => reason
        )
        .increment(1);
    }

    pub fn prospective_outcome_pending() {
        counter!("prospective_outcome_pending_total").increment(1);
    }

    pub fn prospective_outcome_censored() {
        counter!("prospective_outcome_censored_total").increment(1);
    }

    pub fn candidate_expired(chain: Chain, launchpad: Launchpad, reason: &str) {
        let r = match reason {
            "NO_ACTIVITY" => "NO_ACTIVITY",
            "INSUFFICIENT_BUYERS" => "INSUFFICIENT_BUYERS",
            "MARKET_DEAD" => "MARKET_DEAD",
            "MAX_WATCH_AGE" => "MAX_WATCH_AGE",
            "PROTOCOL_ENDED" => "PROTOCOL_ENDED",
            "SECURITY_REJECT" => "SECURITY_REJECT",
            _ => "OTHER",
        };
        counter!(
            "candidate_expired_total",
            "chain" => chain.as_str(),
            "launchpad" => launchpad.as_str(),
            "reason" => r
        )
        .increment(1);
    }

    pub fn record_lags(
        &self,
        raw: &RawEvent,
        persisted_at: DateTime<Utc>,
        ingest_started: Instant,
    ) {
        if let Some(chain_time) = raw.chain_time() {
            let lag = raw
                .observed_at
                .signed_duration_since(chain_time)
                .num_milliseconds()
                .max(0) as f64;
            histogram!("ingest_lag_ms", "chain" => raw.chain().as_str()).record(lag);
        }
        let persist = persisted_at
            .signed_duration_since(raw.observed_at)
            .num_milliseconds()
            .max(0) as f64;
        histogram!("persist_lag_ms", "chain" => raw.chain().as_str()).record(persist);
        let _ = ingest_started;
        let _ = Launchpad::Unknown;
    }
}

fn mode_label(mode: &str) -> &'static str {
    match mode {
        "historical" => "historical",
        "rpc_dev" => "rpc_dev",
        "yellowstone" => "yellowstone",
        "live" => "live",
        _ => "other",
    }
}

fn source_label(source: &str) -> &'static str {
    if source.contains("backfill") {
        "backfill"
    } else if source.contains("ws") || source.contains("yellowstone") || source.contains("live") {
        "live"
    } else {
        "other"
    }
}

pub fn install_prometheus(addr: &str) -> anyhow::Result<()> {
    let socket: std::net::SocketAddr = addr.parse()?;
    metrics_exporter_prometheus::PrometheusBuilder::new()
        .with_http_listener(socket)
        .install()?;
    Ok(())
}
