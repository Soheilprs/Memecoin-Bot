//! Versioned delay / fee / slippage / failure / retry models. Research priors, not fitted.

use serde::{Deserialize, Serialize};

use crate::domain::{Chain, Launchpad};
use crate::state::lifecycle::TokenLifecycleState;

use super::types::{LatencyScenario, OrderSide};

/// Entry/exit delay scenarios. NOT claimed live latencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelayModel {
    pub version: String,
    pub scenario: LatencyScenario,
    pub solana_fast_ms: i64,
    pub solana_base_ms: i64,
    pub solana_slow_ms: i64,
    pub base_fast_ms: i64,
    pub base_base_ms: i64,
    pub base_slow_ms: i64,
    pub rh_fast_ms: i64,
    pub rh_base_ms: i64,
    pub rh_slow_ms: i64,
}

impl DelayModel {
    pub fn research_default(scenario: LatencyScenario) -> Self {
        Self {
            version: super::types::EXECUTION_MODEL_VERSION.into(),
            scenario,
            solana_fast_ms: 500,
            solana_base_ms: 2_000,
            solana_slow_ms: 5_000,
            base_fast_ms: 2_000,
            base_base_ms: 4_000,
            base_slow_ms: 8_000,
            rh_fast_ms: 500,
            rh_base_ms: 1_000,
            rh_slow_ms: 2_000,
        }
    }

    pub fn delay_ms(&self, chain: Chain) -> i64 {
        match (chain, self.scenario) {
            (Chain::Solana, LatencyScenario::Fast) => self.solana_fast_ms,
            (Chain::Solana, LatencyScenario::Base) => self.solana_base_ms,
            (Chain::Solana, LatencyScenario::Slow) => self.solana_slow_ms,
            (Chain::Base, LatencyScenario::Fast) => self.base_fast_ms,
            (Chain::Base, LatencyScenario::Base) => self.base_base_ms,
            (Chain::Base, LatencyScenario::Slow) => self.base_slow_ms,
            (Chain::Robinhood, LatencyScenario::Fast) => self.rh_fast_ms,
            (Chain::Robinhood, LatencyScenario::Base) => self.rh_base_ms,
            (Chain::Robinhood, LatencyScenario::Slow) => self.rh_slow_ms,
        }
    }
}

/// Venue fees with explicit provenance. Scenario values, not empirically fitted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeModel {
    pub version: String,
    pub pump_curve_protocol_bps: u32,
    pub pump_curve_creator_bps: u32,
    pub pumpswap_lp_bps: u32,
    pub pons_curve_bps: u32,
    pub pons_snipe_tax_bps: u32,
    pub pons_snipe_window_ms: i64,
    pub clanker_unknown: bool,
    pub network_fee_quote: String,
    pub priority_fee_quote: String,
    pub tip_quote: String,
    pub provenance: String,
}

impl FeeModel {
    pub fn research_default() -> Self {
        Self {
            version: super::types::FEE_MODEL_VERSION.into(),
            // Pump.fun curve: 1% protocol is the widely documented default; creator share
            // varies by era. Scenario only — see docs/execution-models.md.
            pump_curve_protocol_bps: 100,
            pump_curve_creator_bps: 0,
            // PumpSwap AMM: 25 bps combined LP/protocol scenario (not live-measured).
            pumpswap_lp_bps: 25,
            // Pons V2: 1% curve fee typical (MEMECOIN_BOT_RESEARCH_V2.md).
            pons_curve_bps: 100,
            // Snipe tax is launch-specific; 9900 bps used only when window is active
            // in a test/scenario. Default window 1s; tax 0 unless `snipe_active`.
            pons_snipe_tax_bps: 9900,
            pons_snipe_window_ms: 1_000,
            clanker_unknown: true,
            network_fee_quote: "0".into(),
            priority_fee_quote: "0".into(),
            tip_quote: "0".into(),
            provenance: "phase6_scenario_v6.0.0: pump 100bps curve (documented default); pumpswap 25bps scenario; pons 100bps curve (research_v2); snipe tax 9900bps only inside configured window; clanker hook fee UNKNOWN".into(),
        }
    }

    pub fn protocol_bps(&self, launchpad: Launchpad, life: TokenLifecycleState) -> u32 {
        match launchpad {
            Launchpad::PumpFun => self.pump_curve_protocol_bps + self.pump_curve_creator_bps,
            Launchpad::PumpSwap => self.pumpswap_lp_bps,
            Launchpad::PonsV2 => match life {
                TokenLifecycleState::AmmActive => self.pumpswap_lp_bps,
                _ => self.pons_curve_bps,
            },
            Launchpad::ClankerV4 | Launchpad::Unknown => 0,
        }
    }

    pub fn snipe_tax_bps(&self, launchpad: Launchpad, age_ms: i64, force_snipe: bool) -> u32 {
        if launchpad != Launchpad::PonsV2 {
            return 0;
        }
        if force_snipe || age_ms < self.pons_snipe_window_ms {
            self.pons_snipe_tax_bps
        } else {
            0
        }
    }
}

/// Extra adverse movement while the tx is in flight. Conservative: always against us.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlippageModel {
    pub version: String,
    pub adverse_bps: u32,
}

impl SlippageModel {
    pub fn none() -> Self {
        Self {
            version: super::types::EXECUTION_MODEL_VERSION.into(),
            adverse_bps: 0,
        }
    }
    pub fn bps(adverse_bps: u32) -> Self {
        Self {
            version: super::types::EXECUTION_MODEL_VERSION.into(),
            adverse_bps,
        }
    }

    /// BUY: fewer tokens. SELL: less quote.
    pub fn apply(&self, amount: &str, side: OrderSide) -> String {
        let _ = side;
        apply_adverse_bps(amount, self.adverse_bps)
    }
}

pub fn apply_adverse_bps(amount: &str, bps: u32) -> String {
    use crate::state::amt::{parse_u256, u256_dec};
    if bps == 0 {
        return amount.to_string();
    }
    let a = parse_u256(amount);
    let keep = 10_000u64.saturating_sub(bps as u64);
    u256_dec(
        a.saturating_mul(alloy_primitives::U256::from(keep))
            / alloy_primitives::U256::from(10_000u64),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureModel {
    pub version: String,
    pub entry_failure_bps: u32,
    pub exit_failure_bps: u32,
    pub seed: u64,
}

impl FailureModel {
    pub fn none(seed: u64) -> Self {
        Self {
            version: super::types::FAILURE_MODEL_VERSION.into(),
            entry_failure_bps: 0,
            exit_failure_bps: 0,
            seed,
        }
    }

    pub fn rates(seed: u64, entry_bps: u32, exit_bps: u32) -> Self {
        Self {
            version: super::types::FAILURE_MODEL_VERSION.into(),
            entry_failure_bps: entry_bps,
            exit_failure_bps: exit_bps,
            seed,
        }
    }

    pub fn fails(&self, is_entry: bool, token: &str, time_ms: i64, attempt: u32) -> bool {
        let rate = if is_entry {
            self.entry_failure_bps
        } else {
            self.exit_failure_bps
        };
        if rate == 0 {
            return false;
        }
        deterministic_hit(self.seed, token, time_ms, attempt, rate)
    }
}

pub fn deterministic_hit(
    seed: u64,
    token: &str,
    time_ms: i64,
    attempt: u32,
    rate_bps: u32,
) -> bool {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(seed.to_le_bytes());
    h.update(token.as_bytes());
    h.update(time_ms.to_le_bytes());
    h.update(attempt.to_le_bytes());
    let d = h.finalize();
    let v = u64::from_le_bytes(d[0..8].try_into().unwrap());
    (v % 10_000) < u64::from(rate_bps)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryModel {
    pub version: String,
    pub max_entry_retries: u32,
    pub max_exit_retries: u32,
    pub max_emergency_retries: u32,
    pub retry_delay_ms: i64,
}

impl RetryModel {
    pub fn research_default() -> Self {
        Self {
            version: super::types::EXECUTION_MODEL_VERSION.into(),
            max_entry_retries: 1,
            max_exit_retries: 2,
            max_emergency_retries: 3,
            retry_delay_ms: 400,
        }
    }

    pub fn max_attempts(&self, is_entry: bool, emergency: bool) -> u32 {
        let extra = if is_entry {
            self.max_entry_retries
        } else if emergency {
            self.max_emergency_retries
        } else {
            self.max_exit_retries
        };
        extra.saturating_add(1)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimConfig {
    pub delay: DelayModel,
    pub fees: FeeModel,
    pub slippage: SlippageModel,
    pub failure: FailureModel,
    pub retry: RetryModel,
    pub quote_notional: String,
    pub max_slippage_bps: u32,
    pub allow_snipe_window: bool,
}

impl SimConfig {
    pub fn research_default() -> Self {
        Self {
            delay: DelayModel::research_default(LatencyScenario::Base),
            fees: FeeModel::research_default(),
            slippage: SlippageModel::none(),
            failure: FailureModel::none(1),
            retry: RetryModel::research_default(),
            quote_notional: "1000000000".into(),
            max_slippage_bps: 10_000,
            allow_snipe_window: false,
        }
    }

    pub fn with_latency(mut self, s: LatencyScenario) -> Self {
        self.delay.scenario = s;
        self
    }

    pub fn with_slippage(mut self, bps: u32) -> Self {
        self.slippage = SlippageModel::bps(bps);
        self
    }

    pub fn with_notional(mut self, q: impl Into<String>) -> Self {
        self.quote_notional = q.into();
        self
    }
}
