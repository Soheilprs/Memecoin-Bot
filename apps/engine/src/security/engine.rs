use crate::domain::Chain;
use crate::metrics::DiscoveryMetrics;
use crate::security::assessment::SecurityAssessment;
use crate::security::context::SecurityContext;
use crate::security::policy::SecurityPolicy;

#[derive(Default)]
pub struct SecurityEngine {
    pub policy: SecurityPolicy,
}

impl SecurityEngine {
    pub fn new(policy: SecurityPolicy) -> Self {
        Self { policy }
    }

    /// Shared live + replay analyzer. Missing historical state → UNKNOWN, never current-chain fill.
    pub fn assess(&self, ctx: &SecurityContext) -> SecurityAssessment {
        let started = std::time::Instant::now();
        let mut evidence = match ctx.token.chain {
            Chain::Solana => crate::security::solana::analyze(ctx, &self.policy),
            Chain::Base | Chain::Robinhood => crate::security::evm::analyze(ctx, &self.policy),
        };
        if ctx.data_quality == crate::domain::QualityStatus::RpcDevIncomplete {
            evidence.push(crate::security::evidence::SecurityEvidence::new(
                "DATA_QUALITY",
                crate::security::evidence::EvidenceStatus::Warn,
                crate::security::evidence::Severity::Medium,
                "session",
                "RPC_DEV_INCOMPLETE propagated; analysis does not upgrade completeness",
            ));
        }
        if self.policy.require_sellability {
            let has_sell = evidence.iter().any(|e| e.check == "SELLABILITY");
            if !has_sell {
                evidence.push(crate::security::evidence::SecurityEvidence::new(
                    "SELLABILITY",
                    crate::security::evidence::EvidenceStatus::Unknown,
                    crate::security::evidence::Severity::High,
                    "policy",
                    "require_sellability but no sell probe ran",
                ));
            }
        }
        let mut a = SecurityAssessment::from_evidence(
            ctx.token.chain,
            ctx.token.token_address.clone(),
            ctx.token.launchpad,
            evidence,
            ctx.data_quality,
            ctx.as_of_time,
        );
        a.as_of_block = ctx.as_of_block.map(|b| b as i64);
        a.as_of_slot = ctx.as_of_slot.map(|s| s as i64);
        a.source_session_id = ctx.source_session_id;
        a.snapshot_id = ctx.snapshot_id;
        a.analyzer_version = self.policy.analyzer_version.to_string();
        a.policy_version = self.policy.policy_version.to_string();
        DiscoveryMetrics::security_assessment(a.chain, a.launchpad, a.verdict.as_str());
        DiscoveryMetrics::security_static_latency_ms(started.elapsed().as_millis() as i64);
        a
    }
}
