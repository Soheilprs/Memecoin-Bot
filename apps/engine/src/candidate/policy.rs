/// Research priors only. Not optimized against PnL.
#[derive(Debug, Clone)]
pub struct CandidatePolicy {
    pub policy_id: String,
    pub policy_version: String,
    pub min_confirm_age_ms: i64,
    pub min_trades_for_confirmation: u64,
    pub min_unique_buyers_for_confirmation: u64,
    pub min_eligible_age_ms: i64,
    pub min_trades_for_eligible: u64,
    pub min_unique_buyers_for_eligible: u64,
    pub max_candidate_age_ms: i64,
    pub expire_no_activity_ms: i64,
    pub allow_security_warn: bool,
}

impl CandidatePolicy {
    pub fn research_default() -> Self {
        Self {
            policy_id: "default".into(),
            policy_version: "5.0.0".into(),
            min_confirm_age_ms: 5_000,
            min_trades_for_confirmation: 1,
            min_unique_buyers_for_confirmation: 1,
            min_eligible_age_ms: 15_000,
            min_trades_for_eligible: 3,
            min_unique_buyers_for_eligible: 2,
            max_candidate_age_ms: 3_600_000,
            expire_no_activity_ms: 300_000,
            allow_security_warn: true,
        }
    }

    pub fn conservative() -> Self {
        let mut p = Self::research_default();
        p.policy_id = "conservative".into();
        p.min_trades_for_eligible = 8;
        p.min_unique_buyers_for_eligible = 5;
        p.min_eligible_age_ms = 30_000;
        p
    }
}

impl Default for CandidatePolicy {
    fn default() -> Self {
        Self::research_default()
    }
}
