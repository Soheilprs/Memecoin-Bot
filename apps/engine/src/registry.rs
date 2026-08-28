use crate::domain::{Chain, Launchpad};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    Verified,
    NeedsVerification,
}

impl VerificationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            VerificationStatus::Verified => "verified",
            VerificationStatus::NeedsVerification => "needs_verification",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FactoryRecord {
    pub chain: Chain,
    pub launchpad: Launchpad,
    pub address: &'static str,
    pub verification_status: VerificationStatus,
    pub source: &'static str,
    pub abi_idl_version: &'static str,
    pub enabled: bool,
}

pub const PUMPFUN_PROGRAM: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
pub const PUMPSWAP_PROGRAM: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";
pub const PONS_V2_FACTORY: &str = "0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e";
pub const PONS_V2_HOOK: &str = "0xe5e702641ea86f4ae6cc3cdaed2b886f976be044";
pub const ROBINHOOD_V4_POOL_MANAGER: &str = "0x8366a39cc670b4001a1121b8f6a443a643e40951";
pub const CLANKER_V4_FACTORY: &str = "0xe85a59c628f7d27878aceb4bf3b35733630083a9";
pub const BASE_V4_POOL_MANAGER: &str = "0x498581ff718922c3f8e6a244956af099b2652b2b";
pub const WETH_BASE: &str = "0x4200000000000000000000000000000000000006";
pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";

pub const PUMPFUN_IDL_VERSION: &str = "0.1.0";
pub const PONS_ABI_VERSION: &str = "v2-tokenlaunched-1";
pub const CLANKER_ABI_VERSION: &str = "v4-tokencreated-1";
pub const UNISWAP_V4_ABI_VERSION: &str = "v4-poolmanager-1";

pub fn verified_factories() -> Vec<FactoryRecord> {
    vec![
        FactoryRecord {
            chain: Chain::Solana,
            launchpad: Launchpad::PumpFun,
            address: PUMPFUN_PROGRAM,
            verification_status: VerificationStatus::Verified,
            source: "pump-public-docs IDL",
            abi_idl_version: PUMPFUN_IDL_VERSION,
            enabled: true,
        },
        FactoryRecord {
            chain: Chain::Robinhood,
            launchpad: Launchpad::PonsV2,
            address: PONS_V2_FACTORY,
            verification_status: VerificationStatus::Verified,
            source: "on-chain TokenLaunched 2026-08-27",
            abi_idl_version: PONS_ABI_VERSION,
            enabled: true,
        },
        FactoryRecord {
            chain: Chain::Base,
            launchpad: Launchpad::ClankerV4,
            address: CLANKER_V4_FACTORY,
            verification_status: VerificationStatus::Verified,
            source: "clanker-devco/v4-contracts",
            abi_idl_version: CLANKER_ABI_VERSION,
            enabled: true,
        },
        FactoryRecord {
            chain: Chain::Solana,
            launchpad: Launchpad::PumpSwap,
            address: PUMPSWAP_PROGRAM,
            verification_status: VerificationStatus::Verified,
            source: "pump-public-docs PumpSwap program",
            abi_idl_version: PUMPFUN_IDL_VERSION,
            enabled: true,
        },
        FactoryRecord {
            chain: Chain::Robinhood,
            launchpad: Launchpad::PonsV2,
            address: ROBINHOOD_V4_POOL_MANAGER,
            verification_status: VerificationStatus::Verified,
            source: "Robinhood Uniswap v4 PoolManager",
            abi_idl_version: UNISWAP_V4_ABI_VERSION,
            enabled: true,
        },
        FactoryRecord {
            chain: Chain::Base,
            launchpad: Launchpad::ClankerV4,
            address: BASE_V4_POOL_MANAGER,
            verification_status: VerificationStatus::Verified,
            source: "Base Uniswap v4 PoolManager",
            abi_idl_version: UNISWAP_V4_ABI_VERSION,
            enabled: true,
        },
    ]
}

pub fn lookup_factory(chain: Chain, address: &str) -> Option<FactoryRecord> {
    let needle = normalize_factory_address(chain, address);
    verified_factories()
        .into_iter()
        .find(|f| f.chain == chain && normalize_factory_address(chain, f.address) == needle)
}

pub fn normalize_factory_address(chain: Chain, address: &str) -> String {
    match chain {
        Chain::Solana => address.to_string(),
        Chain::Base | Chain::Robinhood => crate::domain::raw_event::normalize_address(address),
    }
}
