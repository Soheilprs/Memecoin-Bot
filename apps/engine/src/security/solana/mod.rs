pub mod authorities;
pub mod metadata;
pub mod sellability;
pub mod token2022;

use crate::domain::Launchpad;
use crate::registry::{PUMPFUN_PROGRAM, PUMPSWAP_PROGRAM};
use crate::security::context::SecurityContext;
use crate::security::evidence::{EvidenceStatus, SecurityEvidence, Severity};
use crate::security::policy::SecurityPolicy;
use crate::state::lifecycle::TokenLifecycleState;

pub const SPL_TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

pub fn analyze(ctx: &SecurityContext, policy: &SecurityPolicy) -> Vec<SecurityEvidence> {
    let mut ev = Vec::new();
    let pump = ctx.token.launchpad == Launchpad::PumpFun
        || ctx.token.factory_or_program == PUMPFUN_PROGRAM;
    let pumpswap = ctx.token.launchpad == Launchpad::PumpSwap
        || ctx.token.factory_or_program == PUMPSWAP_PROGRAM;

    if pump {
        if ctx.token.factory_or_program == PUMPFUN_PROGRAM {
            ev.push(
                SecurityEvidence::new(
                    "TEMPLATE_PUMPFUN",
                    EvidenceStatus::Pass,
                    Severity::Info,
                    "discovery_provenance",
                    "token discovered via pinned Pump.fun program, not by name/symbol",
                )
                .with_value(PUMPFUN_PROGRAM),
            );
        } else {
            ev.push(
                SecurityEvidence::new(
                    "TEMPLATE_PUMPFUN",
                    EvidenceStatus::Fail,
                    Severity::Critical,
                    "discovery_provenance",
                    "launchpad labeled Pump.fun but factory/program is not the pinned program",
                )
                .with_value(&ctx.token.factory_or_program)
                .reject(),
            );
        }
    }
    if pumpswap {
        ev.push(
            SecurityEvidence::new(
                "TEMPLATE_PUMPSWAP",
                EvidenceStatus::Pass,
                Severity::Info,
                "discovery_provenance",
                "PumpSwap program provenance from discovery",
            )
            .with_value(PUMPSWAP_PROGRAM),
        );
    }

    let program = ctx.solana.as_ref().and_then(|s| s.token_program.as_deref());
    match program {
        Some(p) if p == SPL_TOKEN_PROGRAM => ev.push(SecurityEvidence::new(
            "TOKEN_PROGRAM",
            EvidenceStatus::Pass,
            Severity::Info,
            "account_owner",
            "SPL Token program",
        )),
        Some(p) if p == TOKEN_2022_PROGRAM => ev.push(
            SecurityEvidence::new(
                "TOKEN_PROGRAM",
                EvidenceStatus::Pass,
                Severity::Info,
                "account_owner",
                "Token-2022 (expected for Pump create_v2)",
            )
            .with_value(TOKEN_2022_PROGRAM),
        ),
        Some(p) => {
            let mut e = SecurityEvidence::new(
                "TOKEN_PROGRAM",
                EvidenceStatus::Fail,
                Severity::Critical,
                "account_owner",
                "unknown token program; not interpreted as SPL",
            )
            .with_value(p);
            if policy.reject_unknown_token_program {
                e = e.reject();
            }
            ev.push(e);
        }
        None if ctx.historical => ev.push(SecurityEvidence::new(
            "TOKEN_PROGRAM",
            EvidenceStatus::UnknownHistoricalState,
            Severity::Medium,
            "no_account",
            "token program not in historical view",
        )),
        None => ev.push(SecurityEvidence::new(
            "TOKEN_PROGRAM",
            EvidenceStatus::Unknown,
            Severity::Medium,
            "rpc",
            "token program not loaded",
        )),
    }

    let mint = ctx.solana.as_ref().and_then(|s| s.mint_account.as_deref());
    let missing = mint.is_none();
    let parsed = mint.and_then(authorities::parse_mint_header);
    ev.extend(authorities::assess_authorities(
        parsed.as_ref(),
        program,
        pump,
        ctx.token.curve.as_deref(),
        ctx.historical && missing,
        policy,
    ));
    if let Some(bytes) = mint {
        if program == Some(TOKEN_2022_PROGRAM) || bytes.len() > 82 {
            ev.extend(token2022::assess_extensions(bytes, pump, policy));
        }
    }

    ev.extend(metadata::assess_metadata(None, None));

    let gap = ctx.lifecycle == Some(TokenLifecycleState::GraduationGap);
    let sell = sellability::classify(&sellability::TemplateSellability {
        launchpad_ok: pump || pumpswap,
        graduation_gap: gap,
        rpc_simulate_available: false,
        simulated_ok: None,
    });
    let details = if gap {
        "protocol transition; not labeled honeypot"
    } else if pump || pumpswap {
        "Pump/PumpSwap sell is not proven via Jupiter. simulateTransaction not used (no user key, no broadcast). PROVIDER_LIMITED, not SELLABLE."
    } else {
        "no sell simulation constructed"
    };
    ev.push(sellability::evidence(sell, details));
    ev
}
