use crate::domain::{Chain, Launchpad};
use crate::registry::{
    CLANKER_V4_FACTORY, PONS_V2_FACTORY, PONS_V2_HOOK, ROBINHOOD_V4_POOL_MANAGER,
};
use crate::security::evidence::{EvidenceStatus, SecurityEvidence, Severity};
use crate::security::evm::bytecode::runtime_hash;

/// Surveyed 2026-08-28: 24 factory-matching TokenLaunched tokens, all unique runtime hashes.
pub const PONS_TEMPLATE_REGISTRY_VERSION: &str = "v2-survey-2026-08-28";
pub const PONS_TOKEN_RUNTIME_HASH_STATUS: &str = "PONS_TEMPLATE_HASH_UNPINNED";
/// Factory runtime keccak256 from eth_getCode (not a token-template identity).
pub const PONS_V2_FACTORY_RUNTIME_HASH: &str =
    "0x89a27da6f703e0a7cdd4f233e7cb57604ff75b164530962d3ff7cf8483a67d84";

#[derive(Debug, Clone)]
pub struct TemplateRecord {
    pub chain: Chain,
    pub factory: &'static str,
    pub template_name: &'static str,
    pub launchpad: Launchpad,
    pub runtime_bytecode_hash: Option<&'static str>,
    pub expected_hook: Option<&'static str>,
    pub source: &'static str,
    pub version: &'static str,
}

pub fn known_templates() -> Vec<TemplateRecord> {
    vec![
        TemplateRecord {
            chain: Chain::Base,
            factory: CLANKER_V4_FACTORY,
            template_name: "clanker_v4",
            launchpad: Launchpad::ClankerV4,
            runtime_bytecode_hash: None,
            expected_hook: None,
            source: "clanker-devco/v4-contracts + factory provenance",
            version: "v4-tokencreated-1",
        },
        TemplateRecord {
            chain: Chain::Robinhood,
            factory: PONS_V2_FACTORY,
            template_name: "pons_v2",
            launchpad: Launchpad::PonsV2,
            // Token/curve runtimes embed per-launch immutables; 24/24 recent tokens
            // had distinct keccak256. Not a single template hash. See
            // PONS_TOKEN_RUNTIME_HASH_STATUS.
            runtime_bytecode_hash: None,
            expected_hook: Some(PONS_V2_HOOK),
            source: "on-chain TokenLaunched + eth_getCode survey 2026-08-28; factory hash pinned separately",
            version: PONS_TEMPLATE_REGISTRY_VERSION,
        },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateMatch {
    Strong,
    Mismatch,
    Unpinned,
    None,
}

pub fn match_template(
    chain: Chain,
    launchpad: Launchpad,
    factory: &str,
    bytecode: Option<&[u8]>,
) -> (TemplateMatch, Option<TemplateRecord>, Vec<SecurityEvidence>) {
    let factory_n = factory.to_ascii_lowercase();
    let rec = known_templates().into_iter().find(|t| {
        t.chain == chain && t.launchpad == launchpad && t.factory.eq_ignore_ascii_case(&factory_n)
    });
    let mut ev = Vec::new();
    let Some(rec) = rec else {
        ev.push(SecurityEvidence::new(
            "TEMPLATE_KNOWN",
            EvidenceStatus::NotFound,
            Severity::Info,
            "registry",
            "not a pinned Clanker v4 / Pons V2 factory provenance match",
        ));
        return (TemplateMatch::None, None, ev);
    };
    ev.push(
        SecurityEvidence::new(
            "TEMPLATE_FACTORY",
            EvidenceStatus::Found,
            Severity::Info,
            "discovery_provenance",
            format!(
                "factory matches pinned {} — factory address alone is not safety",
                rec.template_name
            ),
        )
        .with_value(rec.factory),
    );
    if rec.expected_hook == Some(PONS_V2_HOOK) {
        ev.push(
            SecurityEvidence::new(
                "TEMPLATE_PONS_HOOK",
                EvidenceStatus::Found,
                Severity::Info,
                "registry",
                "expected Pons v4 hook relationship recorded",
            )
            .with_value(PONS_V2_HOOK),
        );
        ev.push(
            SecurityEvidence::new(
                "TEMPLATE_PONS_POOL_MANAGER",
                EvidenceStatus::Found,
                Severity::Info,
                "registry",
                "expected Uniswap v4 PoolManager on Robinhood",
            )
            .with_value(ROBINHOOD_V4_POOL_MANAGER),
        );
    }
    match (rec.runtime_bytecode_hash, bytecode) {
        (Some(expect), Some(code)) => {
            let got = runtime_hash(code);
            if got.eq_ignore_ascii_case(expect) {
                ev.push(
                    SecurityEvidence::new(
                        "TEMPLATE_BYTECODE",
                        EvidenceStatus::Pass,
                        Severity::Info,
                        "runtime_keccak256",
                        "runtime hash matches pinned template",
                    )
                    .with_value(got),
                );
                (TemplateMatch::Strong, Some(rec), ev)
            } else {
                ev.push(
                    SecurityEvidence::new(
                        "TEMPLATE_MISMATCH",
                        EvidenceStatus::Fail,
                        Severity::High,
                        "runtime_keccak256",
                        "provenance says known factory but bytecode hash is unexpected; falling through to full analyzer (not automatic PASS)",
                    )
                    .with_value(got),
                );
                (TemplateMatch::Mismatch, Some(rec), ev)
            }
        }
        (_, None) => {
            ev.push(SecurityEvidence::new(
                "TEMPLATE_BYTECODE",
                EvidenceStatus::Warn,
                Severity::Medium,
                "runtime",
                "runtime bytecode not available; factory match is not treated as PASS by itself",
            ));
            (TemplateMatch::Unpinned, Some(rec), ev)
        }
        (None, Some(code)) => {
            ev.push(
                SecurityEvidence::new(
                    "TEMPLATE_BYTECODE",
                    EvidenceStatus::Warn,
                    Severity::Low,
                    "runtime_keccak256",
                    "template runtime hash is not pinned; recorded for later comparison, not treated as proof of safety",
                )
                .with_value(runtime_hash(code)),
            );
            (TemplateMatch::Unpinned, Some(rec), ev)
        }
    }
}
