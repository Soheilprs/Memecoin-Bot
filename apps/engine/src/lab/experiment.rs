use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::{Chain, Launchpad, QualityStatus};
use crate::features::FEATURE_VERSION;
use crate::sim::types::{
    EXECUTION_MODEL_VERSION, FEE_MODEL_VERSION, IMPACT_MODEL_VERSION, OUTCOME_MODEL_VERSION,
};
use crate::strategy::{StrategyThresholds, STRATEGY_POLICY_VERSION};

use super::split::SplitBounds;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExperimentStatus {
    Draft,
    Training,
    Locked,
    Validating,
    Testing,
    Complete,
    Invalid,
}

impl ExperimentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Training => "TRAINING",
            Self::Locked => "LOCKED",
            Self::Validating => "VALIDATING",
            Self::Testing => "TESTING",
            Self::Complete => "COMPLETE",
            Self::Invalid => "INVALID",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyExperiment {
    pub experiment_id: String,
    pub name: String,
    pub description: String,
    pub hypothesis: String,
    pub dataset_id: Option<String>,
    pub dataset_hash: Option<String>,
    pub chain: Option<Chain>,
    pub launchpad: Option<Launchpad>,
    pub splits: Option<SplitBounds>,
    pub feature_version: String,
    pub security_policy_version: String,
    pub candidate_policy_version: String,
    pub strategy_policy_version: String,
    pub execution_model_version: String,
    pub fee_model_version: String,
    pub impact_model_version: String,
    pub slippage_model_version: String,
    pub outcome_model_version: String,
    pub position_size: String,
    pub exit_policy_id: String,
    pub entry_policy_id: String,
    pub thresholds: StrategyThresholds,
    pub config_hash: Option<String>,
    pub locked_config: Option<serde_json::Value>,
    pub status: ExperimentStatus,
    pub variants_evaluated: u32,
    pub hypotheses_tested: u32,
    pub git_commit: Option<String>,
    pub seed: u64,
    pub data_quality: QualityStatus,
    #[serde(default)]
    pub test_run_count: u32,
    pub created_at: DateTime<Utc>,
    pub locked_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl StrategyExperiment {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            experiment_id: id.into(),
            name: name.into(),
            description: String::new(),
            hypothesis: String::new(),
            dataset_id: None,
            dataset_hash: None,
            chain: None,
            launchpad: None,
            splits: None,
            feature_version: FEATURE_VERSION.into(),
            security_policy_version: crate::security::POLICY_VERSION.into(),
            candidate_policy_version: "5.0.0".into(),
            strategy_policy_version: STRATEGY_POLICY_VERSION.into(),
            execution_model_version: EXECUTION_MODEL_VERSION.into(),
            fee_model_version: FEE_MODEL_VERSION.into(),
            impact_model_version: IMPACT_MODEL_VERSION.into(),
            slippage_model_version: EXECUTION_MODEL_VERSION.into(),
            outcome_model_version: OUTCOME_MODEL_VERSION.into(),
            position_size: "1000000000".into(),
            exit_policy_id: "X2_TIME_5M".into(),
            entry_policy_id: "S0_BASELINE".into(),
            thresholds: StrategyThresholds::train_defaults(),
            config_hash: None,
            locked_config: None,
            status: ExperimentStatus::Draft,
            variants_evaluated: 0,
            hypotheses_tested: 8,
            git_commit: None,
            seed: 1,
            data_quality: QualityStatus::HistoricalReplay,
            test_run_count: 0,
            created_at: Utc::now(),
            locked_at: None,
            completed_at: None,
        }
    }

    pub fn lockable_payload(&self) -> serde_json::Value {
        serde_json::json!({
            "entry": self.entry_policy_id,
            "exit": self.exit_policy_id,
            "thresholds": self.thresholds,
            "size": self.position_size,
            "seed": self.seed,
            "exec": self.execution_model_version,
            "fee": self.fee_model_version,
            "impact": self.impact_model_version,
            "slip": self.slippage_model_version,
            "security": self.security_policy_version,
            "candidate": self.candidate_policy_version,
            "strategy": self.strategy_policy_version,
            "splits": self.splits,
            "dataset_hash": self.dataset_hash,
        })
    }

    pub fn lock(&mut self) -> Result<(), &'static str> {
        if matches!(
            self.status,
            ExperimentStatus::Locked | ExperimentStatus::Testing | ExperimentStatus::Complete
        ) {
            return Err("ALREADY_LOCKED");
        }
        let cfg = self.lockable_payload();
        self.config_hash = Some(config_hash(&cfg));
        self.locked_config = Some(cfg);
        self.status = ExperimentStatus::Locked;
        self.locked_at = Some(Utc::now());
        Ok(())
    }

    pub fn verify_lock(&self) -> Result<(), &'static str> {
        let Some(h) = &self.config_hash else {
            return Err("NOT_LOCKED");
        };
        let Some(cfg) = &self.locked_config else {
            return Err("NOT_LOCKED");
        };
        if *h != config_hash(cfg) {
            return Err("HASH_MISMATCH");
        }
        if config_hash(&self.lockable_payload()) != *h {
            return Err("CONFIG_DRIFT");
        }
        Ok(())
    }

    pub fn begin_test(&mut self) -> Result<(), &'static str> {
        self.verify_lock()?;
        if self.test_run_count >= 1 {
            return Err("TEST_ALREADY_RUN");
        }
        self.status = ExperimentStatus::Testing;
        self.test_run_count += 1;
        Ok(())
    }

    pub fn assert_dataset_hash(&self, actual: &str) -> Result<(), &'static str> {
        match &self.dataset_hash {
            None => Err("DATASET_HASH_MISSING"),
            Some(h) if h != actual => Err("DATASET_HASH_MISMATCH"),
            Some(_) => Ok(()),
        }
    }

    /// Locked out-of-sample TEST. Refuses a second run and hash mismatch.
    pub fn begin_out_of_sample_test(
        &mut self,
        actual_dataset_hash: &str,
    ) -> Result<(), &'static str> {
        if !self.data_quality.is_research_complete() {
            return Err("DATASET_NOT_RESEARCH_VALID");
        }
        self.assert_dataset_hash(actual_dataset_hash)?;
        self.begin_test()
    }
}

pub fn config_hash(v: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(v).unwrap_or_default();
    hex::encode(Sha256::digest(bytes))
}

pub fn dataset_hash(token_times: &[(String, i64)]) -> String {
    let mut rows: Vec<_> = token_times
        .iter()
        .map(|(t, ms)| format!("{t}:{ms}"))
        .collect();
    rows.sort();
    hex::encode(Sha256::digest(rows.join("\n").as_bytes()))
}
