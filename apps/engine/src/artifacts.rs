use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::registry::{CLANKER_ABI_VERSION, PONS_ABI_VERSION, PUMPFUN_IDL_VERSION};
use crate::state::PONS_CURVE_ABI_VERSION;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactMeta {
    pub protocol: &'static str,
    pub chain: &'static str,
    pub version: &'static str,
    pub source: &'static str,
    pub retrieved_at: DateTime<Utc>,
    pub sha256: String,
    pub bytes: &'static [u8],
}

pub fn pumpfun_idl_bytes() -> &'static [u8] {
    include_bytes!("../../../crates/programs/solana/pumpfun/idl.json")
}

pub fn pons_abi_bytes() -> &'static [u8] {
    include_bytes!("../../../crates/programs/evm/pons_v2/abi.json")
}

pub fn pons_curve_views_bytes() -> &'static [u8] {
    include_bytes!("../../../crates/programs/evm/pons_v2/curve_views.json")
}

pub fn clanker_abi_bytes() -> &'static [u8] {
    include_bytes!("../../../crates/programs/evm/clanker_v4/abi.json")
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn retrieved_at() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-08-27T00:00:00Z")
        .expect("static timestamp")
        .with_timezone(&Utc)
}

pub fn pumpfun_artifact() -> ArtifactMeta {
    ArtifactMeta {
        protocol: "pumpfun",
        chain: "solana",
        version: PUMPFUN_IDL_VERSION,
        source: "https://raw.githubusercontent.com/pump-fun/pump-public-docs/refs/heads/main/idl/pump.json",
        retrieved_at: retrieved_at(),
        sha256: sha256_hex(pumpfun_idl_bytes()),
        bytes: pumpfun_idl_bytes(),
    }
}

pub fn pons_artifact() -> ArtifactMeta {
    ArtifactMeta {
        protocol: "pons_v2",
        chain: "robinhood",
        version: PONS_ABI_VERSION,
        source: "Bitquery Pons API + on-chain topic0 verification 2026-08-27",
        retrieved_at: retrieved_at(),
        sha256: sha256_hex(pons_abi_bytes()),
        bytes: pons_abi_bytes(),
    }
}

pub fn pons_curve_views_artifact() -> ArtifactMeta {
    ArtifactMeta {
        protocol: "pons_v2_bonding_curve",
        chain: "robinhood",
        version: PONS_CURVE_ABI_VERSION,
        source: "https://github.com/ponsdotdev/ponsfamily/blob/main/contractsV2/src/v2/PonsV2BondingCurve.sol",
        retrieved_at: retrieved_at(),
        sha256: sha256_hex(pons_curve_views_bytes()),
        bytes: pons_curve_views_bytes(),
    }
}

pub fn clanker_artifact() -> ArtifactMeta {
    ArtifactMeta {
        protocol: "clanker_v4",
        chain: "base",
        version: CLANKER_ABI_VERSION,
        source: "https://raw.githubusercontent.com/clanker-devco/v4-contracts/main/src/interfaces/IClanker.sol",
        retrieved_at: retrieved_at(),
        sha256: sha256_hex(clanker_abi_bytes()),
        bytes: clanker_abi_bytes(),
    }
}

pub fn all_artifacts() -> Vec<ArtifactMeta> {
    vec![
        pumpfun_artifact(),
        pons_artifact(),
        pons_curve_views_artifact(),
        clanker_artifact(),
    ]
}

pub fn artifact_for(protocol: &str, version: &str) -> Option<ArtifactMeta> {
    all_artifacts()
        .into_iter()
        .find(|a| a.protocol == protocol && a.version == version)
}
