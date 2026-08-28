use super::TOKEN_2022_PROGRAM;
use crate::security::evidence::{EvidenceStatus, SecurityEvidence, Severity};
use crate::security::policy::SecurityPolicy;

/// SPL / Token-2022 mint header (82 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MintAuthorities {
    pub mint_authority: Option<String>,
    pub freeze_authority: Option<String>,
    pub supply: u64,
    pub decimals: u8,
    pub initialized: bool,
}

pub fn parse_mint_header(data: &[u8]) -> Option<MintAuthorities> {
    if data.len() < 82 {
        return None;
    }
    let mint_opt = u32::from_le_bytes(data[0..4].try_into().ok()?);
    let mint_pk = &data[4..36];
    let supply = u64::from_le_bytes(data[36..44].try_into().ok()?);
    let decimals = data[44];
    let initialized = data[45] != 0;
    let freeze_opt = u32::from_le_bytes(data[46..50].try_into().ok()?);
    let freeze_pk = &data[50..82];
    Some(MintAuthorities {
        mint_authority: (mint_opt != 0).then(|| bs58::encode(mint_pk).into_string()),
        freeze_authority: (freeze_opt != 0).then(|| bs58::encode(freeze_pk).into_string()),
        supply,
        decimals,
        initialized,
    })
}

pub fn encode_mint_header(auth: &MintAuthorities) -> Vec<u8> {
    let mut b = vec![0u8; 82];
    if let Some(pk) = &auth.mint_authority {
        b[0..4].copy_from_slice(&1u32.to_le_bytes());
        let raw = bs58::decode(pk)
            .into_vec()
            .unwrap_or_else(|_| vec![1u8; 32]);
        let mut pk32 = [0u8; 32];
        let n = raw.len().min(32);
        pk32[..n].copy_from_slice(&raw[..n]);
        b[4..36].copy_from_slice(&pk32);
    }
    b[36..44].copy_from_slice(&auth.supply.to_le_bytes());
    b[44] = auth.decimals;
    b[45] = u8::from(auth.initialized);
    if let Some(pk) = &auth.freeze_authority {
        b[46..50].copy_from_slice(&1u32.to_le_bytes());
        let raw = bs58::decode(pk)
            .into_vec()
            .unwrap_or_else(|_| vec![2u8; 32]);
        let mut pk32 = [0u8; 32];
        let n = raw.len().min(32);
        pk32[..n].copy_from_slice(&raw[..n]);
        b[50..82].copy_from_slice(&pk32);
    }
    b
}

pub fn assess_authorities(
    mint: Option<&MintAuthorities>,
    token_program: Option<&str>,
    pump_template: bool,
    bonding_curve: Option<&str>,
    historical_missing: bool,
    policy: &SecurityPolicy,
) -> Vec<SecurityEvidence> {
    let mut out = Vec::new();
    if historical_missing && mint.is_none() {
        out.push(
            SecurityEvidence::new(
                "SOLANA_MINT_AUTHORITY",
                EvidenceStatus::UnknownHistoricalState,
                Severity::Medium,
                "no_mint_account",
                "mint account bytes were not available at the requested slot; current chain state was not substituted",
            ),
        );
        out.push(SecurityEvidence::new(
            "SOLANA_FREEZE_AUTHORITY",
            EvidenceStatus::UnknownHistoricalState,
            Severity::Medium,
            "no_mint_account",
            "freeze authority not observed historically",
        ));
        return out;
    }
    let Some(m) = mint else {
        out.push(
            SecurityEvidence::new(
                "SOLANA_MINT_ACCOUNT",
                EvidenceStatus::Unknown,
                Severity::High,
                "rpc",
                "mint account not loaded",
            )
            .reject(),
        );
        return out;
    };
    let curve = bonding_curve.unwrap_or("");
    match &m.mint_authority {
        None => out.push(
            SecurityEvidence::new(
                "SOLANA_MINT_AUTHORITY",
                EvidenceStatus::Pass,
                Severity::Info,
                "mint_account",
                "mint authority is None (fixed supply as of this account)",
            )
            .with_value("none"),
        ),
        Some(pk) if pump_template && (pk == curve || token_program == Some(TOKEN_2022_PROGRAM)) => {
            out.push(
                SecurityEvidence::new(
                    "SOLANA_MINT_AUTHORITY",
                    EvidenceStatus::Warn,
                    Severity::Low,
                    "mint_account",
                    "Pump.fun create_v2 may keep mint authority on the bonding curve during the curve; not a generic mint backdoor. Exception documented.",
                )
                .with_value(pk.clone()),
            );
        }
        Some(pk) => {
            let mut e = SecurityEvidence::new(
                "SOLANA_MINT_AUTHORITY",
                EvidenceStatus::Fail,
                Severity::Critical,
                "mint_account",
                "active mint authority on a non-template or unexpected account",
            )
            .with_value(pk.clone());
            if policy.reject_arbitrary_mint {
                e = e.reject();
            }
            out.push(e);
        }
    }
    match &m.freeze_authority {
        None => out.push(
            SecurityEvidence::new(
                "SOLANA_FREEZE_AUTHORITY",
                EvidenceStatus::Pass,
                Severity::Info,
                "mint_account",
                "no freeze authority",
            )
            .with_value("none"),
        ),
        Some(pk) if pump_template && pk == curve => out.push(
            SecurityEvidence::new(
                "SOLANA_FREEZE_AUTHORITY",
                EvidenceStatus::Warn,
                Severity::Low,
                "mint_account",
                "freeze authority is the Pump bonding curve (protocol-owned during curve). Exception documented.",
            )
            .with_value(pk.clone()),
        ),
        Some(pk) => {
            let mut e = SecurityEvidence::new(
                "SOLANA_FREEZE_AUTHORITY",
                EvidenceStatus::Fail,
                Severity::Critical,
                "mint_account",
                "active freeze authority can halt transfers",
            )
            .with_value(pk.clone());
            if policy.reject_active_freeze_authority {
                e = e.reject();
            }
            out.push(e);
        }
    }
    out
}
