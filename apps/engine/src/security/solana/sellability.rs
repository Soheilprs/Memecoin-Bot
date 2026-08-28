use async_trait::async_trait;

use crate::error::Result;
use crate::security::assessment::Sellability;
use crate::security::evidence::{EvidenceStatus, SecurityEvidence, Severity};

#[async_trait]
pub trait SellabilityProbe: Send + Sync {
    async fn probe(&self) -> Result<Sellability>;
}

/// Pump.fun / PumpSwap template: sell path is the protocol program, not Jupiter.
/// We do not broadcast and do not use a user key. Without simulateTransaction RPC,
/// status is UNKNOWN / PROVIDER_LIMITED — never SELLABLE from a quote API.
pub struct TemplateSellability {
    pub launchpad_ok: bool,
    pub graduation_gap: bool,
    pub rpc_simulate_available: bool,
    pub simulated_ok: Option<bool>,
}

#[async_trait]
impl SellabilityProbe for TemplateSellability {
    async fn probe(&self) -> Result<Sellability> {
        Ok(classify(self))
    }
}

pub fn classify(t: &TemplateSellability) -> Sellability {
    if t.graduation_gap {
        return Sellability::NotApplicable;
    }
    if t.simulated_ok == Some(false) {
        return Sellability::NotSellable;
    }
    if t.simulated_ok == Some(true) && t.launchpad_ok {
        return Sellability::Sellable;
    }
    if !t.rpc_simulate_available {
        return Sellability::ProviderLimited;
    }
    Sellability::Unknown
}

pub fn evidence(status: Sellability, details: &str) -> SecurityEvidence {
    let (st, sev, reject) = match status {
        Sellability::Sellable => (EvidenceStatus::Pass, Severity::Info, false),
        Sellability::NotSellable => (EvidenceStatus::Fail, Severity::Critical, true),
        Sellability::NotApplicable => (EvidenceStatus::NotApplicable, Severity::Info, false),
        Sellability::ProviderLimited => (EvidenceStatus::ProviderLimited, Severity::Medium, false),
        Sellability::Unknown => (EvidenceStatus::Unknown, Severity::Medium, false),
    };
    let mut e = SecurityEvidence::new("SELLABILITY", st, sev, "solana_probe", details)
        .with_value(status.as_str());
    if reject {
        e = e.reject();
    }
    e
}
