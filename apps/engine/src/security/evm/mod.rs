pub mod bytecode;
pub mod privileges;
pub mod proxy;
pub mod selectors;
pub mod simulation;
pub mod template;
pub mod token_state;

use std::time::Duration;

use crate::security::context::SecurityContext;
use crate::security::evidence::{EvidenceStatus, SecurityEvidence, Severity};
use crate::security::policy::SecurityPolicy;
use crate::state::lifecycle::TokenLifecycleState;

use self::template::TemplateMatch;

pub fn analyze(ctx: &SecurityContext, policy: &SecurityPolicy) -> Vec<SecurityEvidence> {
    let factory = &ctx.token.factory_or_program;
    let code = ctx.evm.as_ref().and_then(|e| e.runtime_bytecode.as_deref());
    let (tm, rec, mut ev) =
        template::match_template(ctx.token.chain, ctx.token.launchpad, factory, code);

    if ctx.lifecycle == Some(TokenLifecycleState::GraduationGap) {
        ev.push(SecurityEvidence::new(
            "PONS_GRADUATION_GAP",
            EvidenceStatus::Warn,
            Severity::High,
            "state_engine",
            "TEMPORARILY_UNSELLABLE_PROTOCOL_TRANSITION (LaunchSwept → PoolGraduated). Not a honeypot; future strategy must not enter.",
        ));
        ev.push(crate::security::solana::sellability::evidence(
            crate::security::assessment::Sellability::NotApplicable,
            "graduation gap",
        ));
    }

    let known = matches!(tm, TemplateMatch::Strong | TemplateMatch::Unpinned);
    let full = matches!(tm, TemplateMatch::Mismatch | TemplateMatch::None);

    if let Some(code) = code {
        ev.push(
            SecurityEvidence::new(
                "EVM_RUNTIME_HASH",
                EvidenceStatus::Found,
                Severity::Info,
                "keccak256",
                "exact runtime keccak256 (verified source is not treated as safety)",
            )
            .with_value(bytecode::runtime_hash(code)),
        );
        ev.push(
            SecurityEvidence::new(
                "EVM_STRIPPED_HASH",
                EvidenceStatus::Found,
                Severity::Info,
                "keccak256",
                "compiler-metadata-stripped hash",
            )
            .with_value(bytecode::stripped_hash(code)),
        );
        let sels = selectors::extract_push4(code);
        ev.push(
            SecurityEvidence::new(
                "EVM_SELECTOR_COUNT",
                EvidenceStatus::Found,
                Severity::Info,
                "bytecode",
                "PUSH4 selectors extracted; a 4-byte collision is not a reject",
            )
            .with_value(sels.len().to_string()),
        );
        let storage = ctx
            .evm
            .as_ref()
            .map(|e| e.storage.as_slice())
            .unwrap_or(&[]);
        ev.extend(proxy::assess_proxy(code, storage, known, policy));
        if full {
            ev.extend(privileges::assess_privileges(code, policy));
        } else {
            ev.push(SecurityEvidence::new(
                "EVM_TEMPLATE_FAST_PATH",
                EvidenceStatus::Pass,
                Severity::Info,
                rec.as_ref()
                    .map(|r| r.template_name)
                    .unwrap_or("template"),
                "pinned Clanker/Pons provenance; full unknown-contract mint/tax scanner skipped. Template-safe ≠ financially safe.",
            ));
        }
    } else if ctx.historical {
        ev.push(SecurityEvidence::new(
            "EVM_RUNTIME_BYTECODE",
            EvidenceStatus::UnknownHistoricalState,
            Severity::Medium,
            "no_bytecode",
            "runtime bytecode not available at as-of block; current chain bytecode was not substituted",
        ));
    } else {
        ev.push(SecurityEvidence::new(
            "EVM_RUNTIME_BYTECODE",
            EvidenceStatus::Unknown,
            Severity::Medium,
            "rpc",
            "runtime bytecode not loaded (UNKNOWN, not PASS)",
        ));
    }

    ev.extend(token_state::assess_missing_getters(code.is_some()));

    let run_sim = full || ctx.sim_model.is_some();
    if run_sim {
        if let Some(model) = &ctx.sim_model {
            let report =
                simulation::run_plan(model, Duration::from_millis(policy.simulation_timeout_ms));
            ev.extend(simulation::evidence_from_report(&report, policy));
        } else if full {
            ev.push(SecurityEvidence::new(
                "HONEYPOT_SIM",
                EvidenceStatus::Unknown,
                Severity::Medium,
                "local_fork",
                "no simulation model and no live fork RPC; UNKNOWN not PASS",
            ));
        }
    }

    ev
}
