use alloy_primitives::{keccak256, B256, U256};

use crate::security::evidence::{EvidenceStatus, SecurityEvidence, Severity};
use crate::security::policy::SecurityPolicy;

const EIP1167_PREFIX: [u8; 10] = [0x36, 0x3d, 0x3d, 0x37, 0x3d, 0x3d, 0x3d, 0x36, 0x3d, 0x73];
const EIP1167_SUFFIX: [u8; 15] = [
    0x5a, 0xf4, 0x3d, 0x82, 0x80, 0x3e, 0x90, 0x3d, 0x91, 0x60, 0x2b, 0x57, 0xfd, 0x5b, 0xf3,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyType {
    None,
    Eip1167,
    Eip1967,
    Beacon,
    Uups,
}

impl ProxyType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Eip1167 => "EIP1167",
            Self::Eip1967 => "EIP1967",
            Self::Beacon => "BEACON",
            Self::Uups => "UUPS",
        }
    }
}

pub fn eip1967_implementation_slot() -> B256 {
    // bytes32(uint256(keccak256('eip1967.proxy.implementation')) - 1)
    let h = keccak256(b"eip1967.proxy.implementation");
    let n = U256::from_be_bytes(h.0).saturating_sub(U256::from(1u64));
    B256::from(n.to_be_bytes::<32>())
}

pub fn eip1967_admin_slot() -> B256 {
    let h = keccak256(b"eip1967.proxy.admin");
    let n = U256::from_be_bytes(h.0).saturating_sub(U256::from(1u64));
    B256::from(n.to_be_bytes::<32>())
}

pub fn eip1967_beacon_slot() -> B256 {
    let h = keccak256(b"eip1967.proxy.beacon");
    let n = U256::from_be_bytes(h.0).saturating_sub(U256::from(1u64));
    B256::from(n.to_be_bytes::<32>())
}

pub fn detect_eip1167(code: &[u8]) -> Option<[u8; 20]> {
    if code.len() < 45 {
        return None;
    }
    if !code.starts_with(&EIP1167_PREFIX) {
        return None;
    }
    if !code[30..].starts_with(&EIP1167_SUFFIX[..10]) && code.len() < 44 {
        return None;
    }
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&code[10..30]);
    Some(addr)
}

pub fn has_uups_selector(code: &[u8]) -> bool {
    let sels = super::selectors::extract_push4(code);
    let u1 = super::selectors::selector("upgradeTo(address)");
    let u2 = super::selectors::selector("upgradeToAndCall(address,bytes)");
    sels.iter().any(|s| *s == u1 || *s == u2)
}

pub fn assess_proxy(
    code: &[u8],
    storage: &[(String, String)],
    known_template: bool,
    policy: &SecurityPolicy,
) -> Vec<SecurityEvidence> {
    let mut out = Vec::new();
    if let Some(addr) = detect_eip1167(code) {
        out.push(
            SecurityEvidence::new(
                "EVM_PROXY_EIP1167",
                EvidenceStatus::Found,
                Severity::Medium,
                "bytecode",
                "EIP-1167 minimal proxy; implementation is immutable in the clone",
            )
            .with_value(format!("0x{}", hex::encode(addr))),
        );
    }
    let impl_slot = format!("0x{}", hex::encode(eip1967_implementation_slot()));
    let admin_slot = format!("0x{}", hex::encode(eip1967_admin_slot()));
    let beacon_slot = format!("0x{}", hex::encode(eip1967_beacon_slot()));
    let get = |slot: &str| {
        storage
            .iter()
            .find(|(k, _)| {
                k.eq_ignore_ascii_case(slot)
                    || k.trim_start_matches("0x") == slot.trim_start_matches("0x")
            })
            .map(|(_, v)| v.clone())
    };
    if let Some(imp) = get(&impl_slot) {
        out.push(
            SecurityEvidence::new(
                "EVM_PROXY_IMPLEMENTATION",
                EvidenceStatus::Found,
                Severity::High,
                "EIP1967_STORAGE",
                "EIP-1967 implementation slot is set",
            )
            .with_value(imp),
        );
    }
    if let Some(admin) = get(&admin_slot) {
        let eoa = !admin.ends_with("0000000000000000000000000000000000000000");
        let mut e = SecurityEvidence::new(
            "EVM_PROXY_ADMIN",
            EvidenceStatus::Found,
            if eoa { Severity::Critical } else { Severity::High },
            "EIP1967_STORAGE",
            if eoa {
                "upgrade admin is a non-zero address; EOA-controlled upgradeability is a hard reject for memes unless a known template"
            } else {
                "EIP-1967 admin slot is zero"
            },
        )
        .with_value(admin);
        if eoa && !known_template && policy.reject_upgradeable_eoa_admin {
            e = e.reject();
        }
        out.push(e);
    }
    if let Some(b) = get(&beacon_slot) {
        out.push(
            SecurityEvidence::new(
                "EVM_PROXY_BEACON",
                EvidenceStatus::Found,
                Severity::High,
                "EIP1967_STORAGE",
                "beacon proxy slot set",
            )
            .with_value(b),
        );
    }
    if has_uups_selector(code) {
        let mut e = SecurityEvidence::new(
            "EVM_PROXY_UUPS",
            EvidenceStatus::Found,
            Severity::Critical,
            "bytecode",
            "upgradeTo/upgradeToAndCall present (UUPS). EOA-upgradable memes are rejected unless template-allowed.",
        );
        if !known_template && policy.reject_upgradeable_eoa_admin {
            e = e.reject();
        }
        out.push(e);
    }
    if super::bytecode::has_op(code, 0xf4)
        && detect_eip1167(code).is_none()
        && get(&impl_slot).is_none()
    {
        out.push(SecurityEvidence::new(
            "EVM_DELEGATECALL",
            EvidenceStatus::Found,
            Severity::Medium,
            "bytecode",
            "DELEGATECALL present; not automatically rejected (proxies use it). Context required.",
        ));
    }
    out
}

pub fn slot_hex(slot: B256) -> String {
    format!("0x{}", hex::encode(slot))
}
