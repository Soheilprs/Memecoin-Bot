use chrono::{DateTime, Utc};

use crate::domain::{Chain, Launchpad};
use crate::features::vector::FeatureVector;
use crate::security::assessment::SecurityVerdict;
use crate::security::SecurityAssessment;
use crate::state::lifecycle::TokenLifecycleState;

use super::policy::CandidatePolicy;
use super::state::CandidateState;

#[derive(Debug, Clone)]
pub struct CandidateInput<'a> {
    pub chain: Chain,
    pub token: &'a str,
    pub launchpad: Launchpad,
    pub age_ms: i64,
    pub as_of_time: DateTime<Utc>,
    pub snapshot_id: Option<i64>,
    pub security: Option<&'a SecurityAssessment>,
    pub features: Option<&'a FeatureVector>,
    pub buy_count: u64,
    pub unique_buyers: u64,
    pub trade_count: u64,
    pub lifecycle: TokenLifecycleState,
    pub time_since_last_trade_ms: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CandidateTransition {
    pub id: Option<i64>,
    pub chain: Chain,
    pub token_address: String,
    pub launchpad: Launchpad,
    pub policy_id: String,
    pub policy_version: String,
    pub from_state: CandidateState,
    pub to_state: CandidateState,
    pub reason: String,
    pub as_of_time: DateTime<Utc>,
    pub snapshot_id: Option<i64>,
    pub security_assessment_id: Option<i64>,
    pub feature_vector_id: Option<i64>,
    pub created_at: DateTime<Utc>,
}

pub struct CandidateEngine {
    pub policy: CandidatePolicy,
}

impl CandidateEngine {
    pub fn new(policy: CandidatePolicy) -> Self {
        Self { policy }
    }

    pub fn default_research() -> Self {
        Self::new(CandidatePolicy::research_default())
    }

    /// Advance the machine. ELIGIBLE is not BUY and creates no order.
    pub fn step(
        &self,
        current: CandidateState,
        input: &CandidateInput<'_>,
    ) -> Option<CandidateTransition> {
        let next = self.next_state(current, input)?;
        if next.0 == current {
            return None;
        }
        Some(self.transition(current, next.0, next.1, input))
    }

    /// Apply successive one-state hops (WATCHING → CONFIRMING → ELIGIBLE) without skipping.
    pub fn step_until_stable(
        &self,
        mut current: CandidateState,
        input: &CandidateInput<'_>,
    ) -> Vec<CandidateTransition> {
        let mut out = Vec::new();
        for _ in 0..8 {
            match self.step(current, input) {
                Some(t) => {
                    current = t.to_state;
                    out.push(t);
                }
                None => break,
            }
        }
        out
    }

    fn transition(
        &self,
        from: CandidateState,
        to: CandidateState,
        reason: String,
        input: &CandidateInput<'_>,
    ) -> CandidateTransition {
        CandidateTransition {
            id: None,
            chain: input.chain,
            token_address: input.token.to_string(),
            launchpad: input.launchpad,
            policy_id: self.policy.policy_id.clone(),
            policy_version: self.policy.policy_version.clone(),
            from_state: from,
            to_state: to,
            reason,
            as_of_time: input.as_of_time,
            snapshot_id: input.snapshot_id,
            security_assessment_id: input.security.and_then(|s| s.id),
            feature_vector_id: input.features.and_then(|f| f.id),
            created_at: Utc::now(),
        }
    }

    pub fn next_state(
        &self,
        current: CandidateState,
        input: &CandidateInput<'_>,
    ) -> Option<(CandidateState, String)> {
        let p = &self.policy;
        let verdict = input.security.map(|s| s.verdict);

        if matches!(verdict, Some(SecurityVerdict::Reject))
            && current != CandidateState::SecurityRejected
        {
            return Some((CandidateState::SecurityRejected, "SECURITY_REJECT".into()));
        }

        if current == CandidateState::SecurityRejected {
            return None;
        }
        if current == CandidateState::Expired {
            return None;
        }

        if matches!(input.lifecycle, TokenLifecycleState::Inactive)
            && current != CandidateState::Expired
        {
            return Some((CandidateState::Expired, "PROTOCOL_ENDED".into()));
        }

        if input.age_ms > p.max_candidate_age_ms {
            let reason = if input.unique_buyers < p.min_unique_buyers_for_eligible {
                "INSUFFICIENT_BUYERS"
            } else {
                "MAX_WATCH_AGE"
            };
            return Some((CandidateState::Expired, reason.into()));
        }
        if input.trade_count == 0 && input.age_ms >= p.expire_no_activity_ms {
            return Some((CandidateState::Expired, "NO_ACTIVITY".into()));
        }
        if let Some(idle) = input.time_since_last_trade_ms {
            if idle >= p.expire_no_activity_ms && input.trade_count > 0 {
                return Some((CandidateState::Expired, "MARKET_DEAD".into()));
            }
        }

        match verdict {
            None => {
                if current == CandidateState::Discovered {
                    return Some((CandidateState::SecurityPending, "AWAITING_SECURITY".into()));
                }
                None
            }
            Some(SecurityVerdict::Unknown) => {
                if matches!(
                    current,
                    CandidateState::Discovered | CandidateState::SecurityPending
                ) {
                    Some((CandidateState::DataIncomplete, "SECURITY_UNKNOWN".into()))
                } else {
                    None
                }
            }
            Some(SecurityVerdict::Pass) | Some(SecurityVerdict::Warn) => {
                if matches!(verdict, Some(SecurityVerdict::Warn)) && !p.allow_security_warn {
                    return Some((
                        CandidateState::DataIncomplete,
                        "SECURITY_WARN_NOT_ALLOWED".into(),
                    ));
                }
                self.progress_watch(current, input)
            }
            Some(SecurityVerdict::Reject) => None,
        }
    }

    fn progress_watch(
        &self,
        current: CandidateState,
        input: &CandidateInput<'_>,
    ) -> Option<(CandidateState, String)> {
        let p = &self.policy;
        let confirm_ready = input.age_ms >= p.min_confirm_age_ms
            && input.trade_count >= p.min_trades_for_confirmation
            && input.unique_buyers >= p.min_unique_buyers_for_confirmation;
        let eligible_ready = input.age_ms >= p.min_eligible_age_ms
            && input.trade_count >= p.min_trades_for_eligible
            && input.unique_buyers >= p.min_unique_buyers_for_eligible;

        match current {
            CandidateState::Discovered
            | CandidateState::SecurityPending
            | CandidateState::DataIncomplete => {
                Some((CandidateState::Watching, "SECURITY_PASS_OR_WARN".into()))
            }
            CandidateState::Watching if confirm_ready => {
                Some((CandidateState::Confirming, "MIN_MARKET_EVIDENCE".into()))
            }
            CandidateState::Confirming if eligible_ready => Some((
                CandidateState::Eligible,
                "MIN_DATA_FOR_STRATEGY_EVAL".into(),
            )),
            _ => None,
        }
    }
}

pub fn get_candidate_at_or_before(
    rows: &[CandidateTransition],
    time: DateTime<Utc>,
) -> Option<&CandidateTransition> {
    rows.iter()
        .filter(|t| t.as_of_time <= time)
        .max_by_key(|t| t.as_of_time)
}
