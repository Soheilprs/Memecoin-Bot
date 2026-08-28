use crate::security::evidence::{EvidenceStatus, SecurityEvidence, Severity};
use crate::security::policy::SecurityPolicy;

/// spl_token_2022::extension::ExtensionType
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionType {
    TransferFeeConfig = 1,
    MintCloseAuthority = 3,
    ConfidentialTransferMint = 4,
    DefaultAccountState = 6,
    ImmutableOwner = 7,
    MemoTransfer = 8,
    NonTransferable = 9,
    InterestBearingConfig = 10,
    PermanentDelegate = 12,
    TransferHook = 14,
    ConfidentialTransferFeeConfig = 16,
    MetadataPointer = 18,
    TokenMetadata = 19,
    GroupPointer = 20,
    GroupMemberPointer = 22,
    Unknown = 255,
}

impl ExtensionType {
    pub fn from_u16(v: u16) -> Self {
        match v {
            1 => Self::TransferFeeConfig,
            3 => Self::MintCloseAuthority,
            4 => Self::ConfidentialTransferMint,
            6 => Self::DefaultAccountState,
            7 => Self::ImmutableOwner,
            8 => Self::MemoTransfer,
            9 => Self::NonTransferable,
            10 => Self::InterestBearingConfig,
            12 => Self::PermanentDelegate,
            14 => Self::TransferHook,
            16 => Self::ConfidentialTransferFeeConfig,
            18 => Self::MetadataPointer,
            19 => Self::TokenMetadata,
            20 => Self::GroupPointer,
            22 => Self::GroupMemberPointer,
            _ => Self::Unknown,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::TransferFeeConfig => "TransferFeeConfig",
            Self::MintCloseAuthority => "MintCloseAuthority",
            Self::ConfidentialTransferMint => "ConfidentialTransfer",
            Self::DefaultAccountState => "DefaultAccountState",
            Self::ImmutableOwner => "ImmutableOwner",
            Self::MemoTransfer => "MemoTransfer",
            Self::NonTransferable => "NonTransferable",
            Self::InterestBearingConfig => "InterestBearingConfig",
            Self::PermanentDelegate => "PermanentDelegate",
            Self::TransferHook => "TransferHook",
            Self::ConfidentialTransferFeeConfig => "ConfidentialTransferFee",
            Self::MetadataPointer => "MetadataPointer",
            Self::TokenMetadata => "TokenMetadata",
            Self::GroupPointer => "GroupPointer",
            Self::GroupMemberPointer => "GroupMemberPointer",
            Self::Unknown => "UnknownExtension",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionPolicy {
    Safe,
    Warn,
    Reject,
    Unknown,
}

pub fn classify_extension(ext: ExtensionType, pump_template: bool) -> ExtensionPolicy {
    match ext {
        ExtensionType::MetadataPointer
        | ExtensionType::TokenMetadata
        | ExtensionType::ImmutableOwner
        | ExtensionType::MemoTransfer
        | ExtensionType::GroupPointer
        | ExtensionType::GroupMemberPointer => ExtensionPolicy::Safe,
        ExtensionType::MintCloseAuthority | ExtensionType::InterestBearingConfig => {
            ExtensionPolicy::Warn
        }
        ExtensionType::TransferFeeConfig => ExtensionPolicy::Warn,
        ExtensionType::ConfidentialTransferMint | ExtensionType::ConfidentialTransferFeeConfig => {
            ExtensionPolicy::Unknown
        }
        ExtensionType::TransferHook
        | ExtensionType::PermanentDelegate
        | ExtensionType::NonTransferable
        | ExtensionType::DefaultAccountState => ExtensionPolicy::Reject,
        ExtensionType::Unknown => {
            if pump_template {
                ExtensionPolicy::Warn
            } else {
                ExtensionPolicy::Unknown
            }
        }
    }
}

pub fn parse_tlv_extensions(mint: &[u8]) -> Vec<(ExtensionType, Vec<u8>)> {
    if mint.len() <= 82 {
        return Vec::new();
    }
    let mut i = 82;
    let mut out = Vec::new();
    while i + 4 <= mint.len() {
        let ty = u16::from_le_bytes([mint[i], mint[i + 1]]);
        let len = u16::from_le_bytes([mint[i + 2], mint[i + 3]]) as usize;
        i += 4;
        if i + len > mint.len() {
            break;
        }
        if ty != 0 {
            out.push((ExtensionType::from_u16(ty), mint[i..i + len].to_vec()));
        }
        i += len;
    }
    out
}

pub fn append_tlv(mint: &mut Vec<u8>, ty: ExtensionType, data: &[u8]) {
    if mint.len() < 82 {
        mint.resize(82, 0);
    }
    mint.extend_from_slice(&(ty as u16).to_le_bytes());
    mint.extend_from_slice(&(data.len() as u16).to_le_bytes());
    mint.extend_from_slice(data);
}

pub fn assess_extensions(
    mint: &[u8],
    pump_template: bool,
    policy: &SecurityPolicy,
) -> Vec<SecurityEvidence> {
    let exts = parse_tlv_extensions(mint);
    if exts.is_empty() {
        return vec![SecurityEvidence::new(
            "TOKEN2022_EXTENSIONS",
            EvidenceStatus::NotFound,
            Severity::Info,
            "mint_account",
            "no Token-2022 TLV extensions after mint header",
        )];
    }
    let mut out = Vec::new();
    for (ty, _) in exts {
        let class = classify_extension(ty, pump_template);
        let check = format!("TOKEN2022_{}", ty.name().to_uppercase());
        match class {
            ExtensionPolicy::Safe => out.push(
                SecurityEvidence::new(
                    check,
                    EvidenceStatus::Pass,
                    Severity::Info,
                    "mint_tlv",
                    format!(
                        "{} is accepted{}",
                        ty.name(),
                        if pump_template && ty == ExtensionType::MetadataPointer {
                            " (expected on Pump create_v2)"
                        } else {
                            ""
                        }
                    ),
                )
                .with_value(ty.name()),
            ),
            ExtensionPolicy::Warn => out.push(
                SecurityEvidence::new(
                    check,
                    EvidenceStatus::Warn,
                    Severity::Medium,
                    "mint_tlv",
                    format!("{} present; not automatically fatal", ty.name()),
                )
                .with_value(ty.name()),
            ),
            ExtensionPolicy::Unknown => out.push(
                SecurityEvidence::new(
                    check,
                    EvidenceStatus::Unknown,
                    Severity::Medium,
                    "mint_tlv",
                    format!("{} cannot be fully interpreted", ty.name()),
                )
                .with_value(ty.name()),
            ),
            ExtensionPolicy::Reject => {
                let fatal = match ty {
                    ExtensionType::TransferHook => policy.reject_transfer_hook,
                    ExtensionType::PermanentDelegate => policy.reject_permanent_delegate,
                    ExtensionType::NonTransferable => policy.reject_non_transferable,
                    _ => true,
                };
                let mut e = SecurityEvidence::new(
                    check,
                    EvidenceStatus::Fail,
                    Severity::Critical,
                    "mint_tlv",
                    format!(
                        "{} is unsafe for a trading bot (can steal, freeze, or block sells)",
                        ty.name()
                    ),
                )
                .with_value(ty.name());
                if fatal {
                    e = e.reject();
                }
                out.push(e);
            }
        }
    }
    out
}
