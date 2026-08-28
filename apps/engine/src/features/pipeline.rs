use crate::candidate::{CandidateEngine, CandidateInput, CandidateState, CandidateTransition};
use crate::metrics::DiscoveryMetrics;
use crate::security::SecurityAssessment;
use crate::state::TokenStateSnapshot;

use super::engine::{latest_security_at, FeatureEngine, FeatureInput};
use super::vector::FeatureVector;

#[derive(Debug, Clone, Default)]
pub struct FeatureBatch {
    pub vectors: Vec<FeatureVector>,
    pub transitions: Vec<CandidateTransition>,
}

/// Point-in-time features + candidate steps for a time-ordered snapshot history.
/// Future snapshots are not visible to earlier vectors.
pub fn process_snapshots(
    snapshots: &[TokenStateSnapshot],
    assessments: &[SecurityAssessment],
    candidate: &CandidateEngine,
) -> FeatureBatch {
    let mut out = FeatureBatch::default();
    let mut state_by_token: std::collections::HashMap<
        (crate::domain::Chain, String),
        CandidateState,
    > = std::collections::HashMap::new();

    for i in 0..snapshots.len() {
        let snap = &snapshots[i];
        let history = &snapshots[..i];
        let sec = latest_security_at(
            assessments,
            snap.chain,
            &snap.token_address,
            snap.snapshot_time,
        );
        let input = FeatureInput::from_history(snap, history, sec);
        let started = std::time::Instant::now();
        let vec = FeatureEngine::compute(input);
        DiscoveryMetrics::feature_vector(vec.chain, vec.launchpad);
        DiscoveryMetrics::feature_compute_latency_ms(started.elapsed().as_millis() as i64);

        let key = (snap.chain, snap.token_address.clone());
        let current = state_by_token
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
            security: sec,
            features: Some(&vec),
            buy_count: snap.buy_count_total,
            unique_buyers: snap.unique_buyers_total,
            trade_count: snap.buy_count_total.saturating_add(snap.sell_count_total),
            lifecycle: snap.lifecycle_state,
            time_since_last_trade_ms: snap.wallet.last_trade_age_ms,
        };
        let steps = candidate.step_until_stable(current, &cin);
        if let Some(last) = steps.last() {
            state_by_token.insert(key, last.to_state);
            for t in &steps {
                DiscoveryMetrics::candidate_transition(t.chain, t.launchpad, t.to_state.as_str());
                if t.to_state == CandidateState::Expired {
                    DiscoveryMetrics::candidate_expired(t.chain, t.launchpad, &t.reason);
                }
            }
        }
        out.transitions.extend(steps);
        out.vectors.push(vec);
    }
    out
}

pub fn write_jsonl<W: std::io::Write>(
    vectors: &[FeatureVector],
    mut w: W,
) -> std::io::Result<usize> {
    let mut n = 0;
    for v in vectors {
        serde_json::to_writer(&mut w, v)?;
        w.write_all(b"\n")?;
        n += 1;
    }
    Ok(n)
}
