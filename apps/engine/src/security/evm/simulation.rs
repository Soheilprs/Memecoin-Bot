//! Isolated local market simulation. No broadcast, no user keys, no real capital.
//! This is a deterministic fork of an empty local chain + explicit token model
//! derived from analysis/fixtures — not a live Anvil mainnet fork.

use std::time::{Duration, Instant};

use crate::security::assessment::HoneypotResult;
use crate::security::evidence::{EvidenceStatus, SecurityEvidence, Severity};
use crate::security::policy::SecurityPolicy;

#[derive(Debug, Clone, Default)]
pub struct TokenSimModel {
    pub buy_tax_bps: u32,
    pub sell_tax_bps: u32,
    pub revert_on_buy: bool,
    pub revert_on_sell: bool,
    pub revert_on_second_sell: bool,
    pub revert_on_other_wallet_sell: bool,
    pub transfer_ok: bool,
}

impl TokenSimModel {
    pub fn normal() -> Self {
        Self {
            transfer_ok: true,
            ..Default::default()
        }
    }

    pub fn honeypot_sell_revert() -> Self {
        Self {
            transfer_ok: true,
            revert_on_sell: true,
            ..Default::default()
        }
    }

    pub fn high_sell_tax(bps: u32) -> Self {
        Self {
            transfer_ok: true,
            sell_tax_bps: bps,
            ..Default::default()
        }
    }

    pub fn second_sell_fails() -> Self {
        Self {
            transfer_ok: true,
            revert_on_second_sell: true,
            ..Default::default()
        }
    }

    pub fn other_wallet_cannot_sell() -> Self {
        Self {
            transfer_ok: true,
            revert_on_other_wallet_sell: true,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct SimStepResult {
    pub step: String,
    pub ok: bool,
    pub quote_spent: u128,
    pub tokens_in: u128,
    pub tokens_out: u128,
    pub quote_received: u128,
    pub revert: bool,
}

#[derive(Debug, Clone)]
pub struct SimulationReport {
    pub honeypot: HoneypotResult,
    pub steps: Vec<SimStepResult>,
    pub effective_buy_tax_bps: u32,
    pub effective_sell_tax_bps: u32,
    pub elapsed: Duration,
    pub fork_block_number: Option<u64>,
    pub fork_block_hash: Option<String>,
    pub timed_out: bool,
}

/// BUY → TRANSFER → SELL 50% → SELL remainder → SELL from wallet B.
pub fn run_plan(model: &TokenSimModel, timeout: Duration) -> SimulationReport {
    let started = Instant::now();
    if timeout.as_millis() == 0 {
        return SimulationReport {
            honeypot: HoneypotResult::SimulationFailed,
            steps: Vec::new(),
            effective_buy_tax_bps: 0,
            effective_sell_tax_bps: 0,
            elapsed: Duration::ZERO,
            fork_block_number: Some(0),
            fork_block_hash: Some(
                "0x0000000000000000000000000000000000000000000000000000000000000000".into(),
            ),
            timed_out: true,
        };
    }
    let quote_in: u128 = 1_000_000_000_000_000_000;
    let mut steps = Vec::new();

    let buy_ok = !model.revert_on_buy;
    let tokens = if buy_ok {
        quote_in * (10_000 - model.buy_tax_bps as u128) / 10_000
    } else {
        0
    };
    steps.push(SimStepResult {
        step: "BUY".into(),
        ok: buy_ok,
        quote_spent: quote_in,
        tokens_in: 0,
        tokens_out: tokens,
        quote_received: 0,
        revert: !buy_ok,
    });

    let half = tokens / 2;
    let xfer_ok = buy_ok && model.transfer_ok;
    steps.push(SimStepResult {
        step: "TRANSFER_TO_B".into(),
        ok: xfer_ok,
        quote_spent: 0,
        tokens_in: half,
        tokens_out: if xfer_ok { half } else { 0 },
        quote_received: 0,
        revert: !xfer_ok,
    });

    let sell50_ok = buy_ok && !model.revert_on_sell;
    let quote50 = if sell50_ok {
        half * (10_000 - model.sell_tax_bps as u128) / 10_000
    } else {
        0
    };
    steps.push(SimStepResult {
        step: "SELL_50".into(),
        ok: sell50_ok,
        quote_spent: 0,
        tokens_in: half,
        tokens_out: 0,
        quote_received: quote50,
        revert: !sell50_ok,
    });

    let sell_rest_ok = sell50_ok && !model.revert_on_second_sell;
    let quote_rest = if sell_rest_ok {
        (tokens - half) * (10_000 - model.sell_tax_bps as u128) / 10_000
    } else {
        0
    };
    steps.push(SimStepResult {
        step: "SELL_REMAINDER".into(),
        ok: sell_rest_ok,
        quote_spent: 0,
        tokens_in: tokens - half,
        tokens_out: 0,
        quote_received: quote_rest,
        revert: !sell_rest_ok,
    });

    let b_ok = xfer_ok && !model.revert_on_sell && !model.revert_on_other_wallet_sell;
    let quote_b = if b_ok {
        half * (10_000 - model.sell_tax_bps as u128) / 10_000
    } else {
        0
    };
    steps.push(SimStepResult {
        step: "SELL_FROM_B".into(),
        ok: b_ok,
        quote_spent: 0,
        tokens_in: half,
        tokens_out: 0,
        quote_received: quote_b,
        revert: !b_ok,
    });

    let honeypot = if !buy_ok {
        HoneypotResult::Unknown
    } else if model.revert_on_sell {
        HoneypotResult::Honeypot
    } else if model.revert_on_second_sell || model.revert_on_other_wallet_sell {
        HoneypotResult::Conditional
    } else if model.sell_tax_bps >= 9_000 {
        HoneypotResult::Honeypot
    } else {
        HoneypotResult::NotHoneypot
    };

    SimulationReport {
        honeypot,
        steps,
        effective_buy_tax_bps: model.buy_tax_bps,
        effective_sell_tax_bps: model.sell_tax_bps,
        elapsed: started.elapsed(),
        fork_block_number: Some(0),
        fork_block_hash: Some(
            "0xlocal000000000000000000000000000000000000000000000000000000000001".into(),
        ),
        timed_out: false,
    }
}

pub fn evidence_from_report(
    report: &SimulationReport,
    policy: &SecurityPolicy,
) -> Vec<SecurityEvidence> {
    let mut out = Vec::new();
    if report.timed_out {
        out.push(SecurityEvidence::new(
            "HONEYPOT_SIM",
            EvidenceStatus::Unknown,
            Severity::High,
            "local_fork",
            "simulation timeout → UNKNOWN, not PASS",
        ));
        return out;
    }
    match report.honeypot {
        HoneypotResult::Honeypot => out.push(
            SecurityEvidence::new(
                "HONEYPOT_SIM",
                EvidenceStatus::Fail,
                Severity::Critical,
                "local_fork",
                "buy succeeded but sell reverts or ≥90% disappears as of this isolated fork; not a claim the token can never change",
            )
            .reject(),
        ),
        HoneypotResult::Conditional => out.push(
            SecurityEvidence::new(
                "HONEYPOT_SIM",
                EvidenceStatus::Warn,
                Severity::Critical,
                "local_fork",
                "first sell or first wallet can sell; subsequent or second-wallet sell fails (conditional honeypot)",
            )
            .reject(),
        ),
        HoneypotResult::NotHoneypot => out.push(SecurityEvidence::new(
            "HONEYPOT_SIM",
            EvidenceStatus::Pass,
            Severity::Info,
            "local_fork",
            format!(
                "SELLABLE AS OF isolated fork block {:?}; owner may change state later",
                report.fork_block_number
            ),
        )),
        _ => out.push(SecurityEvidence::new(
            "HONEYPOT_SIM",
            EvidenceStatus::Unknown,
            Severity::Medium,
            "local_fork",
            "simulation failed or inconclusive",
        )),
    }
    if report.effective_sell_tax_bps > policy.max_sell_tax_bps {
        out.push(
            SecurityEvidence::new(
                "EVM_SELL_TAX_BPS",
                EvidenceStatus::Fail,
                Severity::Critical,
                "local_fork",
                format!(
                    "effective sell tax {} bps exceeds max_sell_tax_bps {}",
                    report.effective_sell_tax_bps, policy.max_sell_tax_bps
                ),
            )
            .with_value(report.effective_sell_tax_bps.to_string())
            .reject(),
        );
    }
    if report.effective_buy_tax_bps > policy.max_buy_tax_bps {
        out.push(
            SecurityEvidence::new(
                "EVM_BUY_TAX_BPS",
                EvidenceStatus::Fail,
                Severity::High,
                "local_fork",
                format!(
                    "effective buy tax {} bps exceeds max_buy_tax_bps {}",
                    report.effective_buy_tax_bps, policy.max_buy_tax_bps
                ),
            )
            .reject(),
        );
    }
    for s in &report.steps {
        out.push(
            SecurityEvidence::new(
                format!("SIM_STEP_{}", s.step),
                if s.ok {
                    EvidenceStatus::Pass
                } else {
                    EvidenceStatus::Fail
                },
                if s.ok { Severity::Info } else { Severity::High },
                "local_fork",
                format!(
                    "ok={} revert={} quote_in={} tokens_out={} quote_out={}",
                    s.ok, s.revert, s.quote_spent, s.tokens_out, s.quote_received
                ),
            )
            .with_value(s.step.clone()),
        );
    }
    out
}

pub fn timeout_unknown() -> SimulationReport {
    run_plan(&TokenSimModel::normal(), Duration::from_millis(0))
}
