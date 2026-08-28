//! Explicit security policy. Defaults are provisional safety limits, not tuned scores.

#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    pub analyzer_version: &'static str,
    pub policy_version: &'static str,
    pub reject_active_freeze_authority: bool,
    pub reject_unknown_token_program: bool,
    pub reject_upgradeable_eoa_admin: bool,
    pub reject_arbitrary_mint: bool,
    pub reject_transfer_hook: bool,
    pub reject_permanent_delegate: bool,
    pub reject_non_transferable: bool,
    pub require_sellability: bool,
    pub max_buy_tax_bps: u32,
    pub max_sell_tax_bps: u32,
    pub simulation_timeout_ms: u64,
    pub queue_capacity: usize,
}

impl SecurityPolicy {
    pub const ANALYZER_VERSION: &'static str = "4.0.0";
    pub const POLICY_VERSION: &'static str = "4.0.0";

    pub fn phase4_defaults() -> Self {
        Self {
            analyzer_version: Self::ANALYZER_VERSION,
            policy_version: Self::POLICY_VERSION,
            reject_active_freeze_authority: true,
            reject_unknown_token_program: true,
            reject_upgradeable_eoa_admin: true,
            reject_arbitrary_mint: true,
            reject_transfer_hook: true,
            reject_permanent_delegate: true,
            reject_non_transferable: true,
            require_sellability: false,
            max_buy_tax_bps: 1_000,
            max_sell_tax_bps: 1_000,
            simulation_timeout_ms: 5_000,
            queue_capacity: 256,
        }
    }
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self::phase4_defaults()
    }
}
