use std::collections::HashMap;
use std::sync::Mutex;

use crate::domain::raw_event::normalize_address;
use crate::domain::{Chain, Launchpad};

#[derive(Debug, Clone)]
pub struct MarketRef {
    pub chain: Chain,
    pub launchpad: Launchpad,
    pub token_address: String,
    pub curve: Option<String>,
    pub pool: Option<String>,
    pub pool_id: Option<String>,
    pub quote_asset: Option<String>,
}

#[derive(Default)]
pub struct MarketRegistry {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    by_curve: HashMap<(Chain, String), MarketRef>,
    by_pool: HashMap<(Chain, String), MarketRef>,
    by_token: HashMap<(Chain, String), MarketRef>,
}

impl MarketRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, market: MarketRef) {
        let mut inner = self.inner.lock().unwrap();
        let token_key = (
            market.chain,
            normalize_key(market.chain, &market.token_address),
        );
        if let Some(curve) = market.curve.as_ref() {
            inner.by_curve.insert(
                (market.chain, normalize_key(market.chain, curve)),
                market.clone(),
            );
        }
        if let Some(pool) = market.pool.as_ref() {
            inner.by_pool.insert(
                (market.chain, normalize_key(market.chain, pool)),
                market.clone(),
            );
        }
        if let Some(pool_id) = market.pool_id.as_ref() {
            inner.by_pool.insert(
                (market.chain, normalize_key(market.chain, pool_id)),
                market.clone(),
            );
        }
        inner.by_token.insert(token_key, market);
    }

    pub fn by_curve(&self, chain: Chain, curve: &str) -> Option<MarketRef> {
        let inner = self.inner.lock().unwrap();
        inner
            .by_curve
            .get(&(chain, normalize_key(chain, curve)))
            .cloned()
    }

    pub fn by_pool(&self, chain: Chain, pool: &str) -> Option<MarketRef> {
        let inner = self.inner.lock().unwrap();
        inner
            .by_pool
            .get(&(chain, normalize_key(chain, pool)))
            .cloned()
    }

    pub fn by_token(&self, chain: Chain, token: &str) -> Option<MarketRef> {
        let inner = self.inner.lock().unwrap();
        inner
            .by_token
            .get(&(chain, normalize_key(chain, token)))
            .cloned()
    }

    pub fn knows_pool(&self, chain: Chain, pool: &str) -> bool {
        self.by_pool(chain, pool).is_some()
    }

    pub fn solana_pools(&self) -> Vec<String> {
        let inner = self.inner.lock().unwrap();
        inner
            .by_pool
            .iter()
            .filter(|(k, v)| k.0 == Chain::Solana && v.pool.is_some())
            .filter_map(|(_, v)| v.pool.clone())
            .collect()
    }

    pub fn load_all(&self, markets: impl IntoIterator<Item = MarketRef>) {
        for m in markets {
            self.register(m);
        }
    }
}

fn normalize_key(chain: Chain, value: &str) -> String {
    match chain {
        Chain::Solana => value.to_string(),
        Chain::Base | Chain::Robinhood => normalize_address(value),
    }
}
