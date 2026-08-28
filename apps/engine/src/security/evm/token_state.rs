use crate::security::evidence::{EvidenceStatus, SecurityEvidence, Severity};

/// Missing getters are UNKNOWN, never zero.
pub fn assess_missing_getters(has_bytecode: bool) -> Vec<SecurityEvidence> {
    if !has_bytecode {
        return vec![SecurityEvidence::new(
            "EVM_TOKEN_GETTERS",
            EvidenceStatus::Unknown,
            Severity::Medium,
            "state_read",
            "no runtime bytecode; tax/pause/blacklist getters not read (UNKNOWN, not 0)",
        )];
    }
    vec![SecurityEvidence::new(
        "EVM_TOKEN_GETTERS",
        EvidenceStatus::Unknown,
        Severity::Info,
        "state_read",
        "on-chain getters (buyTax, sellTax, tradingEnabled) not queried in this offline assessment",
    )]
}
