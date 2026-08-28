use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{Chain, Launchpad, QualityStatus};

use super::evidence::{SecurityEvidence, Severity};

pub const ANALYZER_VERSION: &str = super::policy::SecurityPolicy::ANALYZER_VERSION;
pub const POLICY_VERSION: &str = super::policy::SecurityPolicy::POLICY_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SecurityVerdict {
    Pass,
    Warn,
    Reject,
    Unknown,
}

impl SecurityVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Reject => "REJECT",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// UNKNOWN is never PASS.
    pub fn is_pass(self) -> bool {
        matches!(self, Self::Pass)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskBand {
    None,
    Low,
    Medium,
    High,
    Critical,
    Unknown,
}

impl RiskBand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn raise(self, other: Self) -> Self {
        fn rank(b: RiskBand) -> u8 {
            match b {
                RiskBand::None => 0,
                RiskBand::Low => 1,
                RiskBand::Medium => 2,
                RiskBand::High => 3,
                RiskBand::Critical => 4,
                RiskBand::Unknown => 2,
            }
        }
        if rank(other) > rank(self) {
            other
        } else {
            self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Sellability {
    Sellable,
    NotSellable,
    Unknown,
    ProviderLimited,
    NotApplicable,
}

impl Sellability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sellable => "SELLABLE",
            Self::NotSellable => "NOT_SELLABLE",
            Self::Unknown => "UNKNOWN",
            Self::ProviderLimited => "PROVIDER_LIMITED",
            Self::NotApplicable => "NOT_APPLICABLE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HoneypotResult {
    NotHoneypot,
    Honeypot,
    Conditional,
    Unknown,
    SimulationFailed,
}

impl HoneypotResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotHoneypot => "NOT_HONEYPOT",
            Self::Honeypot => "HONEYPOT",
            Self::Conditional => "CONDITIONAL",
            Self::Unknown => "UNKNOWN",
            Self::SimulationFailed => "SIMULATION_FAILED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAssessment {
    pub id: Option<i64>,
    pub chain: Chain,
    pub token_address: String,
    pub launchpad: Launchpad,
    pub as_of_block: Option<i64>,
    pub as_of_block_hash: Option<String>,
    pub as_of_slot: Option<i64>,
    pub as_of_time: DateTime<Utc>,
    pub source_session_id: Option<i64>,
    pub snapshot_id: Option<i64>,
    pub verdict: SecurityVerdict,
    pub contract_risk: RiskBand,
    pub token_mechanics_risk: RiskBand,
    pub privilege_risk: RiskBand,
    pub sellability_risk: RiskBand,
    pub liquidity_structure_risk: RiskBand,
    pub hard_reject_reasons: Vec<String>,
    pub warnings: Vec<String>,
    pub evidence: Vec<SecurityEvidence>,
    pub sellability: Sellability,
    pub honeypot: HoneypotResult,
    pub analyzer_version: String,
    pub policy_version: String,
    pub created_at: DateTime<Utc>,
    pub data_quality: QualityStatus,
    pub fork_block_number: Option<u64>,
    pub fork_block_hash: Option<String>,
}

impl SecurityAssessment {
    pub fn from_evidence(
        chain: Chain,
        token: impl Into<String>,
        launchpad: Launchpad,
        evidence: Vec<SecurityEvidence>,
        data_quality: QualityStatus,
        as_of_time: DateTime<Utc>,
    ) -> Self {
        let mut hard = Vec::new();
        let mut warnings = Vec::new();
        let mut contract = RiskBand::None;
        let mut mechanics = RiskBand::None;
        let mut privilege = RiskBand::None;
        let mut sell = RiskBand::None;
        let mut liq = RiskBand::None;
        let mut unknown = false;
        let mut honeypot = HoneypotResult::Unknown;
        let mut sellability = Sellability::Unknown;

        for e in &evidence {
            if e.hard_reject {
                hard.push(format!("{}: {}", e.check, e.details));
            } else if matches!(e.status, super::evidence::EvidenceStatus::Warn) {
                warnings.push(format!("{}: {}", e.check, e.details));
            }
            if matches!(
                e.status,
                super::evidence::EvidenceStatus::Unknown
                    | super::evidence::EvidenceStatus::UnknownHistoricalState
                    | super::evidence::EvidenceStatus::ProviderLimited
            ) {
                unknown = true;
            }
            let band = match e.severity {
                Severity::Critical => RiskBand::Critical,
                Severity::High => RiskBand::High,
                Severity::Medium => RiskBand::Medium,
                Severity::Low => RiskBand::Low,
                Severity::Info => RiskBand::None,
            };
            if e.check.contains("PROXY")
                || e.check.contains("BYTECODE")
                || e.check.contains("DELEGATE")
                || e.check.contains("TEMPLATE")
            {
                contract = contract.raise(band);
            } else if e.check.contains("TAX")
                || e.check.contains("MINT")
                || e.check.contains("BLACKLIST")
                || e.check.contains("MAX_")
                || e.check.contains("HOOK")
                || e.check.contains("TOKEN2022")
                || e.check.contains("NON_TRANSFERABLE")
            {
                mechanics = mechanics.raise(band);
            } else if e.check.contains("OWNER")
                || e.check.contains("ROLE")
                || e.check.contains("AUTHORITY")
                || e.check.contains("PRIVILEGE")
                || e.check.contains("UPGRADE")
                || e.check.contains("ADMIN")
            {
                privilege = privilege.raise(band);
            } else if e.check.contains("SELL") || e.check.contains("HONEYPOT") {
                sell = sell.raise(band);
            } else if e.check.contains("LIQ") || e.check.contains("POOL") || e.check.contains("GAP")
            {
                liq = liq.raise(band);
            }
            if e.check == "HONEYPOT_SIM" {
                honeypot = match e.status {
                    super::evidence::EvidenceStatus::Fail => HoneypotResult::Honeypot,
                    super::evidence::EvidenceStatus::Pass => HoneypotResult::NotHoneypot,
                    super::evidence::EvidenceStatus::Warn => HoneypotResult::Conditional,
                    super::evidence::EvidenceStatus::Unknown
                    | super::evidence::EvidenceStatus::ProviderLimited => {
                        HoneypotResult::SimulationFailed
                    }
                    _ => honeypot,
                };
            }
            if e.check == "SELLABILITY" {
                sellability = match e.status {
                    super::evidence::EvidenceStatus::Pass => Sellability::Sellable,
                    super::evidence::EvidenceStatus::Fail => Sellability::NotSellable,
                    super::evidence::EvidenceStatus::NotApplicable => Sellability::NotApplicable,
                    super::evidence::EvidenceStatus::ProviderLimited => {
                        Sellability::ProviderLimited
                    }
                    _ => Sellability::Unknown,
                };
            }
        }

        let verdict = if !hard.is_empty() {
            SecurityVerdict::Reject
        } else if unknown && hard.is_empty() && warnings.is_empty() && contract == RiskBand::None {
            // required checks missing → UNKNOWN, never PASS
            SecurityVerdict::Unknown
        } else if unknown && hard.is_empty() {
            // mixed: warnings plus unknowns still not PASS
            if warnings.is_empty() {
                SecurityVerdict::Unknown
            } else {
                SecurityVerdict::Warn
            }
        } else if !warnings.is_empty()
            || mechanics == RiskBand::High
            || privilege == RiskBand::High
            || contract == RiskBand::High
        {
            SecurityVerdict::Warn
        } else {
            SecurityVerdict::Pass
        };

        Self {
            id: None,
            chain,
            token_address: token.into(),
            launchpad,
            as_of_block: None,
            as_of_block_hash: None,
            as_of_slot: None,
            as_of_time,
            source_session_id: None,
            snapshot_id: None,
            verdict,
            contract_risk: contract,
            token_mechanics_risk: mechanics,
            privilege_risk: privilege,
            sellability_risk: sell,
            liquidity_structure_risk: liq,
            hard_reject_reasons: hard,
            warnings,
            evidence,
            sellability,
            honeypot,
            analyzer_version: ANALYZER_VERSION.into(),
            policy_version: POLICY_VERSION.into(),
            created_at: Utc::now(),
            data_quality,
            fork_block_number: None,
            fork_block_hash: None,
        }
    }
}

pub fn format_assessment(a: &SecurityAssessment) -> String {
    let mut out = format!("VERDICT: {}\n\n", a.verdict.as_str());
    if !a.hard_reject_reasons.is_empty() {
        out.push_str("Reasons:\n");
        for r in &a.hard_reject_reasons {
            out.push_str(&format!("- {r}\n"));
        }
        out.push('\n');
    }
    if !a.warnings.is_empty() {
        out.push_str("Warnings:\n");
        for r in &a.warnings {
            out.push_str(&format!("- {r}\n"));
        }
        out.push('\n');
    }
    out.push_str("Evidence:\n");
    for e in &a.evidence {
        out.push_str(&format!(
            "- {} status={} severity={} hard_reject={} value={} source={} :: {}\n",
            e.check,
            e.status.as_str(),
            e.severity.as_str(),
            e.hard_reject,
            e.value.as_deref().unwrap_or("-"),
            e.source,
            e.details
        ));
    }
    out.push_str(&format!(
        "\nanalyzer={} policy={} quality={} sellability={} honeypot={}\n",
        a.analyzer_version,
        a.policy_version,
        a.data_quality.as_str(),
        a.sellability.as_str(),
        a.honeypot.as_str()
    ));
    out
}
