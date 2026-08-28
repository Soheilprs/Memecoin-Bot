use crate::security::evidence::{EvidenceStatus, SecurityEvidence, Severity};
use crate::security::policy::SecurityPolicy;

use super::selectors::labeled_present;

pub fn assess_privileges(code: &[u8], policy: &SecurityPolicy) -> Vec<SecurityEvidence> {
    let hits = labeled_present(code);
    let mut out = Vec::new();
    let kinds: Vec<&str> = hits.iter().map(|h| h.1).collect();
    let has = |k: &str| kinds.contains(&k);

    if has("owner") {
        out.push(SecurityEvidence::new(
            "EVM_OWNER_SELECTOR",
            EvidenceStatus::Found,
            Severity::Medium,
            "bytecode_selectors",
            "owner()/transferOwnership/renounceOwnership selectors present. owner()==0 is not proof of renouncement.",
        ));
    }
    if has("mint") {
        let mut e = SecurityEvidence::new(
            "EVM_MINT_BACKDOOR",
            EvidenceStatus::Found,
            Severity::Critical,
            "bytecode_selectors",
            "mint(address,uint256) selector present; privileged supply inflation is a hard reject except known fixed-issuance templates",
        );
        if policy.reject_arbitrary_mint {
            e = e.reject();
        }
        out.push(e);
    }
    if has("blacklist") {
        out.push(
            SecurityEvidence::new(
                "EVM_BLACKLIST",
                EvidenceStatus::Found,
                Severity::Critical,
                "bytecode_selectors",
                "blacklist control detected; owner can make a wallet unsellable. Hard reject.",
            )
            .reject(),
        );
    }
    if has("tax") {
        out.push(SecurityEvidence::new(
            "EVM_MUTABLE_TAX",
            EvidenceStatus::Found,
            Severity::High,
            "bytecode_selectors",
            "setTax/setSellTax/setFees present. Current tax 0% is not safety; max possible tax is owner-controlled (MUTABILITY_RISK).",
        ));
    }
    if has("max_tx") || has("max_wallet") {
        out.push(SecurityEvidence::new(
            "EVM_MAX_TX_WALLET",
            EvidenceStatus::Found,
            Severity::High,
            "bytecode_selectors",
            "mutable maxTx/maxWallet can block exits",
        ));
    }
    if has("pause") {
        out.push(SecurityEvidence::new(
            "EVM_PAUSE",
            EvidenceStatus::Found,
            Severity::High,
            "bytecode_selectors",
            "pause/unpause can halt selling",
        ));
    }
    if has("upgrade") {
        out.push(SecurityEvidence::new(
            "EVM_UPGRADER_ROLE",
            EvidenceStatus::Found,
            Severity::Critical,
            "bytecode_selectors",
            "upgrade selectors present",
        ));
    }
    if has("role") {
        out.push(SecurityEvidence::new(
            "EVM_ACCESSCONTROL",
            EvidenceStatus::Found,
            Severity::Medium,
            "bytecode_selectors",
            "AccessControl grantRole/revokeRole present; owner()==0 does not clear other roles (fake renounce risk)",
        ));
    }
    if has("owner") && (has("tax") || has("mint") || has("blacklist") || has("upgrade")) {
        out.push(SecurityEvidence::new(
            "EVM_FAKE_RENOUNCE_RISK",
            EvidenceStatus::Found,
            Severity::High,
            "bytecode_selectors",
            "privileged mutators remain even if owner is later zeroed; other roles/setters can still alter trading",
        ));
    }
    if hits.is_empty() {
        out.push(SecurityEvidence::new(
            "EVM_PRIVILEGE_SELECTORS",
            EvidenceStatus::NotFound,
            Severity::Info,
            "bytecode_selectors",
            "no known privilege selectors extracted (not proof they are absent)",
        ));
    }
    out
}
