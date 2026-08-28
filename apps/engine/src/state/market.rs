use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarketStateQuality {
    Complete,
    Partial,
    Unknown,
}

impl MarketStateQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "COMPLETE",
            Self::Partial => "PARTIAL",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarketState {
    BondingCurve(BondingCurveState),
    ConstantProduct(ConstantProductState),
    UniswapV4(UniswapV4State),
    Unknown,
}

impl MarketState {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::BondingCurve(_) => "BONDING_CURVE",
            Self::ConstantProduct(_) => "CONSTANT_PRODUCT",
            Self::UniswapV4(_) => "UNISWAP_V4",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn quality(&self) -> MarketStateQuality {
        match self {
            Self::BondingCurve(s) => s.quality,
            Self::ConstantProduct(s) => s.quality,
            Self::UniswapV4(s) => {
                if s.sqrt_price_x96.is_some() {
                    MarketStateQuality::Complete
                } else {
                    MarketStateQuality::Partial
                }
            }
            Self::Unknown => MarketStateQuality::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BondingCurveState {
    pub virtual_token_reserves: Option<String>,
    pub virtual_sol_reserves: Option<String>,
    pub real_token_reserves: Option<String>,
    pub real_sol_reserves: Option<String>,
    pub token_total_supply: Option<String>,
    pub curve_progress_bps: Option<u32>,
    pub last_token_amount_raw: Option<String>,
    pub last_quote_amount_raw: Option<String>,
    pub quality: MarketStateQuality,
}

impl Default for BondingCurveState {
    fn default() -> Self {
        Self {
            virtual_token_reserves: None,
            virtual_sol_reserves: None,
            real_token_reserves: None,
            real_sol_reserves: None,
            token_total_supply: None,
            curve_progress_bps: None,
            last_token_amount_raw: None,
            last_quote_amount_raw: None,
            quality: MarketStateQuality::Partial,
        }
    }
}

/// PumpSwap / constant-product. Reserves only if present in events — never fabricated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstantProductState {
    pub pool: Option<String>,
    pub token: Option<String>,
    pub quote_asset: Option<String>,
    pub reserve_token_raw: Option<String>,
    pub reserve_quote_raw: Option<String>,
    pub last_token_amount_raw: Option<String>,
    pub last_quote_amount_raw: Option<String>,
    pub quality: MarketStateQuality,
}

impl Default for ConstantProductState {
    fn default() -> Self {
        Self {
            pool: None,
            token: None,
            quote_asset: None,
            reserve_token_raw: None,
            reserve_quote_raw: None,
            last_token_amount_raw: None,
            last_quote_amount_raw: None,
            quality: MarketStateQuality::Partial,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UniswapV4State {
    pub pool_id: Option<String>,
    pub token0: Option<String>,
    pub token1: Option<String>,
    pub quote_asset: Option<String>,
    pub sqrt_price_x96: Option<String>,
    pub liquidity_raw: Option<String>,
    pub tick: Option<String>,
    pub amount0: Option<String>,
    pub amount1: Option<String>,
}
