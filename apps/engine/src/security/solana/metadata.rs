use crate::security::evidence::{EvidenceStatus, SecurityEvidence, Severity};

pub fn assess_metadata(
    update_authority: Option<&str>,
    mutable: Option<bool>,
) -> Vec<SecurityEvidence> {
    let mut out = Vec::new();
    match mutable {
        Some(true) => out.push(
            SecurityEvidence::new(
                "SOLANA_METADATA_MUTABLE",
                EvidenceStatus::Warn,
                Severity::Low,
                "metadata",
                "metadata is mutable; identity can change. WARN only.",
            )
            .with_value("true"),
        ),
        Some(false) => out.push(SecurityEvidence::new(
            "SOLANA_METADATA_MUTABLE",
            EvidenceStatus::Pass,
            Severity::Info,
            "metadata",
            "metadata marked immutable",
        )),
        None => out.push(SecurityEvidence::new(
            "SOLANA_METADATA_MUTABLE",
            EvidenceStatus::Unknown,
            Severity::Info,
            "metadata",
            "metadata mutability not observed",
        )),
    }
    if let Some(auth) = update_authority {
        out.push(
            SecurityEvidence::new(
                "SOLANA_METADATA_UPDATE_AUTHORITY",
                EvidenceStatus::Warn,
                Severity::Low,
                "metadata",
                "update authority present; not a hard reject by itself",
            )
            .with_value(auth),
        );
    }
    out
}
