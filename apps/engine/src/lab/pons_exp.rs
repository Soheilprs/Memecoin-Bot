//! PONS_PROSPECTIVE_EXP001 lock, arms, and operational status. Paper only.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::features::FEATURE_VERSION;
use crate::security::evm::template::{
    PONS_TEMPLATE_REGISTRY_VERSION, PONS_TOKEN_RUNTIME_HASH_STATUS, PONS_V2_FACTORY_RUNTIME_HASH,
};
use crate::security::policy::SecurityPolicy;
use crate::sim::types::{
    EXECUTION_MODEL_VERSION, FEE_MODEL_VERSION, IMPACT_MODEL_VERSION, OUTCOME_MODEL_VERSION,
};
use crate::state::PONS_CURVE_ABI_VERSION;
use crate::strategy::STRATEGY_POLICY_VERSION;

pub const EXP001_ID: &str = "PONS_PROSPECTIVE_EXP001";
pub const EXP002_ID: &str = "PONS_PROSPECTIVE_EXP002";
pub const EXP002_QUAL_ID: &str = "PONS_PROSPECTIVE_EXP002_QUAL";
pub const EXP002_EXITQUAL_ID: &str = "PONS_PROSPECTIVE_EXP002_EXITQUAL";
pub const EXP003_ID: &str = "PONS_PROSPECTIVE_EXP003";
pub const EXP003_RESTARTQUAL_ID: &str = "PONS_PROSPECTIVE_EXP003_RESTARTQUAL";
pub const EXP004_RPCQUAL_ID: &str = "PONS_PROSPECTIVE_EXP004_RPCQUAL";
pub const EXP004_ID: &str = "PONS_PROSPECTIVE_EXP004";
pub const EXP001_MODE: &str = "PROSPECTIVE_PAPER";
pub const CANDIDATE_POLICY_VERSION: &str = "5.0.0";
pub const CURVE_STATE_VERSION: &str = "7.4.0";
pub const POSITION_SIZE: &str = "1000000000";
pub const MIN_DAYS: i64 = 7;
pub const PREFERRED_DAYS: i64 = 14;
pub const VALID_COVERAGE_THRESHOLD: f64 = 0.95;
pub const HEARTBEAT_SECS: i64 = 30;

pub const ENTRY_POLICIES: [&str; 5] = [
    "P0_FIRST_ELIGIBLE_CONTROL",
    "P1_SOLANA_BUYERS_3_30S",
    "P2_SOLANA_BUYERS_PLUS_IMBALANCE",
    "P3_PRICE_WITHOUT_BUYERS_AVOID",
    "P4_LOW_PARTICIPATION_FILTER",
];

pub const EXIT_POLICIES: [&str; 3] = ["X2_TIME_5M", "X6_PARTIAL_RUNNER", "X9_DYNAMIC_RUNNER"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExpRunStatus {
    Preflight,
    Locked,
    Running,
    PausedOperational,
    MinimumComplete,
    PreferredComplete,
    Invalidated,
}

impl ExpRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preflight => "PREFLIGHT",
            Self::Locked => "LOCKED",
            Self::Running => "RUNNING",
            Self::PausedOperational => "PAUSED_OPERATIONAL",
            Self::MinimumComplete => "MINIMUM_COMPLETE",
            Self::PreferredComplete => "PREFERRED_COMPLETE",
            Self::Invalidated => "INVALIDATED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "PREFLIGHT" => Self::Preflight,
            "LOCKED" => Self::Locked,
            "RUNNING" => Self::Running,
            "PAUSED_OPERATIONAL" => Self::PausedOperational,
            "MINIMUM_COMPLETE" => Self::MinimumComplete,
            "PREFERRED_COMPLETE" => Self::PreferredComplete,
            "INVALIDATED" => Self::Invalidated,
            _ => return None,
        })
    }
}

pub fn arm_id(entry: &str, exit: &str) -> String {
    arm_id_for(EXP001_ID, entry, exit)
}

pub fn arm_id_for(experiment_id: &str, entry: &str, exit: &str) -> String {
    format!("{experiment_id}:{entry}:{exit}")
}

pub fn is_research_arm(strategy_policy_id: &str) -> bool {
    strategy_policy_id.starts_with("PONS_PROSPECTIVE_EXP")
        && !strategy_policy_id.contains("PIPELINE_SMOKE")
}

pub fn experiment_prefix(strategy_policy_id: &str) -> Option<&str> {
    let parts: Vec<_> = strategy_policy_id.split(':').collect();
    if parts.len() >= 3 && parts[0].starts_with("PONS_PROSPECTIVE_EXP") {
        Some(parts[0])
    } else {
        None
    }
}

/// SQL LIKE for one experiment's arms. Colon-terminated so
/// `PONS_PROSPECTIVE_EXP002` does not match `PONS_PROSPECTIVE_EXP002_QUAL`.
pub fn experiment_arm_like(experiment_id: &str) -> String {
    format!("{experiment_id}:%")
}

pub fn arm_belongs_to(strategy_policy_id: &str, experiment_id: &str) -> bool {
    strategy_policy_id.starts_with(&format!("{experiment_id}:"))
}

pub fn prospective_entry_eligible(
    discovered_at: DateTime<Utc>,
    feature_as_of: DateTime<Utc>,
    started_at: DateTime<Utc>,
) -> bool {
    discovered_at >= started_at && feature_as_of >= discovered_at && feature_as_of >= started_at
}

pub fn parse_exit_policy(strategy_policy_id: &str) -> &'static str {
    if let Some((_, exit)) = strategy_policy_id.rsplit_once(':') {
        for e in EXIT_POLICIES {
            if e == exit {
                return e;
            }
        }
    }
    "X1_TIME_2M"
}

pub fn is_exp001_arm(strategy_policy_id: &str) -> bool {
    arm_belongs_to(strategy_policy_id, EXP001_ID)
}

pub fn all_arms() -> Vec<String> {
    all_arms_for(EXP001_ID)
}

pub fn all_arms_for(experiment_id: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(15);
    for e in ENTRY_POLICIES {
        for x in EXIT_POLICIES {
            out.push(arm_id_for(experiment_id, e, x));
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exp001Lock {
    pub experiment_id: String,
    pub mode: String,
    pub chain: String,
    pub chain_id: u64,
    pub launchpad: String,
    pub split: String,
    pub policies: Vec<String>,
    pub exits: Vec<String>,
    pub arms: Vec<String>,
    pub position_size: String,
    pub latency: String,
    pub rh_base_ms: i64,
    pub slippage_adverse_bps: u32,
    pub max_slippage_bps: u32,
    pub failure_entry_bps: u32,
    pub failure_exit_bps: u32,
    pub retry_max_entry: u32,
    pub snipe_window_ms: i64,
    pub snipe_tax_bps: u32,
    pub max_total_trade_fee_bps: u32,
    pub pons_curve_abi: String,
    pub curve_state_version: String,
    pub feature_version: String,
    pub security_policy_version: String,
    pub template_registry_version: String,
    pub pons_token_runtime_hash_status: String,
    pub pons_factory_runtime_hash: String,
    pub candidate_policy_version: String,
    pub strategy_policy_version: String,
    pub execution_model_version: String,
    pub fee_model_version: String,
    pub impact_model_version: String,
    pub outcome_model_version: String,
    pub allow_security_warn: bool,
    pub min_days: i64,
    pub preferred_days: i64,
    #[serde(default = "default_coverage")]
    pub valid_coverage_threshold: f64,
    #[serde(default)]
    pub reentry: bool,
    #[serde(default)]
    pub source_tree_hash: Option<String>,
}

fn default_coverage() -> f64 {
    VALID_COVERAGE_THRESHOLD
}

impl Exp001Lock {
    pub fn predeclared() -> Self {
        Self::predeclared_for(EXP001_ID)
    }

    pub fn predeclared_for(experiment_id: &str) -> Self {
        Self {
            experiment_id: experiment_id.into(),
            mode: EXP001_MODE.into(),
            chain: "robinhood".into(),
            chain_id: 4663,
            launchpad: "pons_v2".into(),
            split: "PROSPECTIVE_TEST".into(),
            policies: ENTRY_POLICIES.iter().map(|s| (*s).to_string()).collect(),
            exits: EXIT_POLICIES.iter().map(|s| (*s).to_string()).collect(),
            arms: all_arms_for(experiment_id),
            position_size: POSITION_SIZE.into(),
            latency: "BASE".into(),
            rh_base_ms: 1_000,
            slippage_adverse_bps: 0,
            max_slippage_bps: 10_000,
            failure_entry_bps: 0,
            failure_exit_bps: 0,
            retry_max_entry: 1,
            snipe_window_ms: 1_000,
            snipe_tax_bps: 9_900,
            max_total_trade_fee_bps: 2_000,
            pons_curve_abi: PONS_CURVE_ABI_VERSION.into(),
            curve_state_version: CURVE_STATE_VERSION.into(),
            feature_version: FEATURE_VERSION.into(),
            security_policy_version: SecurityPolicy::POLICY_VERSION.into(),
            template_registry_version: PONS_TEMPLATE_REGISTRY_VERSION.into(),
            pons_token_runtime_hash_status: PONS_TOKEN_RUNTIME_HASH_STATUS.into(),
            pons_factory_runtime_hash: PONS_V2_FACTORY_RUNTIME_HASH.into(),
            candidate_policy_version: CANDIDATE_POLICY_VERSION.into(),
            strategy_policy_version: STRATEGY_POLICY_VERSION.into(),
            execution_model_version: EXECUTION_MODEL_VERSION.into(),
            fee_model_version: FEE_MODEL_VERSION.into(),
            impact_model_version: IMPACT_MODEL_VERSION.into(),
            outcome_model_version: OUTCOME_MODEL_VERSION.into(),
            allow_security_warn: true,
            min_days: MIN_DAYS,
            preferred_days: PREFERRED_DAYS,
            valid_coverage_threshold: VALID_COVERAGE_THRESHOLD,
            reentry: false,
            source_tree_hash: Some(source_tree_hash()),
        }
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::json!({}))
    }

    pub fn config_hash(&self) -> String {
        canonical_sha256(&self.to_value())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exp001State {
    pub lock: Exp001Lock,
    pub config_hash: String,
    pub run_status: ExpRunStatus,
    pub git_commit: Option<String>,
    pub locked_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub start_block: Option<u64>,
    pub start_block_hash: Option<String>,
    pub start_wall_time: Option<DateTime<Utc>>,
    pub restarts: u64,
    pub valid_uptime_secs: i64,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub pause_reason: Option<String>,
}

impl Exp001State {
    pub fn locked(git_commit: Option<String>) -> Self {
        Self::locked_for(EXP001_ID, git_commit)
    }

    pub fn locked_for(experiment_id: &str, git_commit: Option<String>) -> Self {
        let lock = Exp001Lock::predeclared_for(experiment_id);
        let hash = lock.config_hash();
        Self {
            lock,
            config_hash: hash,
            run_status: ExpRunStatus::Locked,
            git_commit,
            locked_at: Some(Utc::now()),
            started_at: None,
            start_block: None,
            start_block_hash: None,
            start_wall_time: None,
            restarts: 0,
            valid_uptime_secs: 0,
            last_heartbeat: None,
            pause_reason: None,
        }
    }

    pub fn verify_lock(&self) -> Result<(), &'static str> {
        if self.config_hash != self.lock.config_hash() {
            return Err("HASH_MISMATCH");
        }
        if !self.lock.experiment_id.starts_with("PONS_PROSPECTIVE_EXP") {
            return Err("WRONG_EXPERIMENT");
        }
        Ok(())
    }
}

pub fn canonical_sha256(v: &serde_json::Value) -> String {
    hex::encode(Sha256::digest(canonical_json(v).as_bytes()))
}

pub fn canonical_json(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let inner: Vec<String> = keys
                .into_iter()
                .map(|k| format!("\"{k}\":{}", canonical_json(&map[&k])))
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        serde_json::Value::Array(arr) => {
            let inner: Vec<String> = arr.iter().map(canonical_json).collect();
            format!("[{}]", inner.join(","))
        }
        other => serde_json::to_string(other).unwrap_or_else(|_| "null".into()),
    }
}

pub fn source_tree_hash() -> String {
    use std::collections::BTreeMap;
    let roots = [
        "apps/engine/src",
        "sql/migrations",
        "crates/programs",
        "research/PONS_PROSPECTIVE_EXP001_SPEC.md",
        "research/PONS_PROSPECTIVE_EXP002_SPEC.md",
    ];
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for root in roots {
        let p = std::path::Path::new(root);
        if p.is_file() {
            if let Ok(b) = std::fs::read(p) {
                files.insert(root.replace('\\', "/"), b);
            }
            continue;
        }
        if !p.is_dir() {
            continue;
        }
        let mut stack = vec![p.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|ext| matches!(ext, "rs" | "sql" | "json" | "md"))
                {
                    if let Ok(b) = std::fs::read(&path) {
                        files.insert(path.to_string_lossy().replace('\\', "/"), b);
                    }
                }
            }
        }
    }
    let mut h = Sha256::new();
    for (k, v) in files {
        h.update(k.as_bytes());
        h.update([0u8]);
        h.update(&v);
    }
    hex::encode(h.finalize())
}

pub fn git_commit() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Exp001StatusReport {
    pub experiment_id: String,
    pub status: String,
    pub config_hash: Option<String>,
    pub git_commit: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub start_block: Option<u64>,
    pub current_block: Option<u64>,
    pub elapsed_secs: i64,
    pub valid_uptime_secs: i64,
    pub restarts: u64,
    pub tokens: i64,
    pub signals: i64,
    pub orders: i64,
    pub fills: i64,
    pub positions_open: i64,
    pub positions_closed: i64,
    pub outcomes_pending: i64,
    pub outcomes_mature: i64,
    pub outcomes_censored: i64,
    pub note: String,
    #[serde(default)]
    pub entry_orders: i64,
    #[serde(default)]
    pub entry_fills: i64,
    #[serde(default)]
    pub exit_orders: i64,
    #[serde(default)]
    pub exit_fills: i64,
    #[serde(default)]
    pub partial_exit_fills: i64,
    #[serde(default)]
    pub failed_exit_attempts: i64,
    #[serde(default)]
    pub positions_opened: i64,
    #[serde(default)]
    pub positions_currently_open: i64,
    #[serde(default)]
    pub session_ended_open: i64,
}
