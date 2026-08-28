use std::collections::{HashMap, VecDeque};

use alloy_primitives::U256;
use serde::{Deserialize, Serialize};

use super::amt::{add_raw, max_raw, median_raw, min_raw, net_signed, parse_u256, sub_sat_raw};

pub const WINDOW_MS: [i64; 7] = [5_000, 15_000, 30_000, 60_000, 120_000, 300_000, 900_000];

#[derive(Debug, Clone)]
pub struct TradeTick {
    pub time_ms: i64,
    pub trader: String,
    pub is_buy: bool,
    pub is_creator: bool,
    pub quote_raw: String,
    pub token_raw: String,
    pub first_buy: bool,
    pub first_sell: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RollingWindowSnapshot {
    pub duration_ms: i64,
    pub buy_count: u64,
    pub sell_count: u64,
    pub unique_buyers: u64,
    pub unique_sellers: u64,
    pub buy_quote_volume_raw: String,
    pub sell_quote_volume_raw: String,
    pub buy_token_volume_raw: String,
    pub sell_token_volume_raw: String,
    pub net_quote_flow: String,
    pub new_unique_buyers: u64,
    pub new_unique_sellers: u64,
    pub creator_buy_volume: String,
    pub creator_sell_volume: String,
    pub trade_size_min: Option<String>,
    pub trade_size_max: Option<String>,
    pub trade_size_median: Option<String>,
}

impl RollingWindowSnapshot {
    fn empty(duration_ms: i64) -> Self {
        Self {
            duration_ms,
            buy_quote_volume_raw: "0".into(),
            sell_quote_volume_raw: "0".into(),
            buy_token_volume_raw: "0".into(),
            sell_token_volume_raw: "0".into(),
            net_quote_flow: "0".into(),
            creator_buy_volume: "0".into(),
            creator_sell_volume: "0".into(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
struct Window {
    duration_ms: i64,
    events: VecDeque<TradeTick>,
    buy_count: u64,
    sell_count: u64,
    buyers: HashMap<String, u32>,
    sellers: HashMap<String, u32>,
    buy_quote: String,
    sell_quote: String,
    buy_token: String,
    sell_token: String,
    creator_buy: String,
    creator_sell: String,
    new_buyers: u64,
    new_sellers: u64,
}

impl Window {
    fn new(duration_ms: i64) -> Self {
        Self {
            duration_ms,
            events: VecDeque::new(),
            buy_count: 0,
            sell_count: 0,
            buyers: HashMap::new(),
            sellers: HashMap::new(),
            buy_quote: "0".into(),
            sell_quote: "0".into(),
            buy_token: "0".into(),
            sell_token: "0".into(),
            creator_buy: "0".into(),
            creator_sell: "0".into(),
            new_buyers: 0,
            new_sellers: 0,
        }
    }

    fn expire(&mut self, now_ms: i64) {
        while let Some(front) = self.events.front() {
            if now_ms.saturating_sub(front.time_ms) >= self.duration_ms {
                let e = self.events.pop_front().unwrap();
                self.remove(&e);
            } else {
                break;
            }
        }
    }

    fn push(&mut self, tick: TradeTick) {
        self.add(&tick);
        self.events.push_back(tick);
    }

    fn add(&mut self, e: &TradeTick) {
        if e.is_buy {
            self.buy_count += 1;
            *self.buyers.entry(e.trader.clone()).or_insert(0) += 1;
            self.buy_quote = add_raw(&self.buy_quote, &e.quote_raw);
            self.buy_token = add_raw(&self.buy_token, &e.token_raw);
            if e.first_buy {
                self.new_buyers += 1;
            }
            if e.is_creator {
                self.creator_buy = add_raw(&self.creator_buy, &e.quote_raw);
            }
        } else {
            self.sell_count += 1;
            *self.sellers.entry(e.trader.clone()).or_insert(0) += 1;
            self.sell_quote = add_raw(&self.sell_quote, &e.quote_raw);
            self.sell_token = add_raw(&self.sell_token, &e.token_raw);
            if e.first_sell {
                self.new_sellers += 1;
            }
            if e.is_creator {
                self.creator_sell = add_raw(&self.creator_sell, &e.quote_raw);
            }
        }
    }

    fn remove(&mut self, e: &TradeTick) {
        if e.is_buy {
            self.buy_count = self.buy_count.saturating_sub(1);
            if let Some(c) = self.buyers.get_mut(&e.trader) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    self.buyers.remove(&e.trader);
                }
            }
            self.buy_quote = sub_sat_raw(&self.buy_quote, &e.quote_raw);
            self.buy_token = sub_sat_raw(&self.buy_token, &e.token_raw);
            if e.first_buy {
                self.new_buyers = self.new_buyers.saturating_sub(1);
            }
            if e.is_creator {
                self.creator_buy = sub_sat_raw(&self.creator_buy, &e.quote_raw);
            }
        } else {
            self.sell_count = self.sell_count.saturating_sub(1);
            if let Some(c) = self.sellers.get_mut(&e.trader) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    self.sellers.remove(&e.trader);
                }
            }
            self.sell_quote = sub_sat_raw(&self.sell_quote, &e.quote_raw);
            self.sell_token = sub_sat_raw(&self.sell_token, &e.token_raw);
            if e.first_sell {
                self.new_sellers = self.new_sellers.saturating_sub(1);
            }
            if e.is_creator {
                self.creator_sell = sub_sat_raw(&self.creator_sell, &e.quote_raw);
            }
        }
    }

    fn snapshot(&self) -> RollingWindowSnapshot {
        let mut sizes: Vec<U256> = self
            .events
            .iter()
            .map(|e| parse_u256(&e.quote_raw))
            .collect();
        let mut min: Option<String> = None;
        let mut max: Option<String> = None;
        for e in &self.events {
            min = Some(match &min {
                None => e.quote_raw.clone(),
                Some(m) => min_raw(m, &e.quote_raw),
            });
            max = Some(match &max {
                None => e.quote_raw.clone(),
                Some(m) => max_raw(m, &e.quote_raw),
            });
        }
        RollingWindowSnapshot {
            duration_ms: self.duration_ms,
            buy_count: self.buy_count,
            sell_count: self.sell_count,
            unique_buyers: self.buyers.len() as u64,
            unique_sellers: self.sellers.len() as u64,
            buy_quote_volume_raw: self.buy_quote.clone(),
            sell_quote_volume_raw: self.sell_quote.clone(),
            buy_token_volume_raw: self.buy_token.clone(),
            sell_token_volume_raw: self.sell_token.clone(),
            net_quote_flow: net_signed(&self.buy_quote, &self.sell_quote),
            new_unique_buyers: self.new_buyers,
            new_unique_sellers: self.new_sellers,
            creator_buy_volume: self.creator_buy.clone(),
            creator_sell_volume: self.creator_sell.clone(),
            trade_size_min: min,
            trade_size_max: max,
            trade_size_median: median_raw(&mut sizes),
        }
    }

    fn snapshot_as_of(&self, at_ms: i64) -> RollingWindowSnapshot {
        let events: Vec<&TradeTick> = self
            .events
            .iter()
            .filter(|e| e.time_ms <= at_ms && at_ms.saturating_sub(e.time_ms) < self.duration_ms)
            .collect();
        let mut buy_count = 0u64;
        let mut sell_count = 0u64;
        let mut buyers = HashMap::<String, u32>::new();
        let mut sellers = HashMap::<String, u32>::new();
        let mut buy_quote = "0".to_string();
        let mut sell_quote = "0".to_string();
        let mut buy_token = "0".to_string();
        let mut sell_token = "0".to_string();
        let mut creator_buy = "0".to_string();
        let mut creator_sell = "0".to_string();
        let mut sizes: Vec<U256> = Vec::new();
        let mut min: Option<String> = None;
        let mut max: Option<String> = None;
        for e in &events {
            sizes.push(parse_u256(&e.quote_raw));
            min = Some(match &min {
                None => e.quote_raw.clone(),
                Some(m) => min_raw(m, &e.quote_raw),
            });
            max = Some(match &max {
                None => e.quote_raw.clone(),
                Some(m) => max_raw(m, &e.quote_raw),
            });
            if e.is_buy {
                buy_count += 1;
                *buyers.entry(e.trader.clone()).or_insert(0) += 1;
                buy_quote = add_raw(&buy_quote, &e.quote_raw);
                buy_token = add_raw(&buy_token, &e.token_raw);
                if e.is_creator {
                    creator_buy = add_raw(&creator_buy, &e.quote_raw);
                }
            } else {
                sell_count += 1;
                *sellers.entry(e.trader.clone()).or_insert(0) += 1;
                sell_quote = add_raw(&sell_quote, &e.quote_raw);
                sell_token = add_raw(&sell_token, &e.token_raw);
                if e.is_creator {
                    creator_sell = add_raw(&creator_sell, &e.quote_raw);
                }
            }
        }
        RollingWindowSnapshot {
            duration_ms: self.duration_ms,
            buy_count,
            sell_count,
            unique_buyers: buyers.len() as u64,
            unique_sellers: sellers.len() as u64,
            buy_quote_volume_raw: buy_quote.clone(),
            sell_quote_volume_raw: sell_quote.clone(),
            buy_token_volume_raw: buy_token,
            sell_token_volume_raw: sell_token,
            net_quote_flow: net_signed(&buy_quote, &sell_quote),
            new_unique_buyers: events.iter().filter(|e| e.is_buy && e.first_buy).count() as u64,
            new_unique_sellers: events.iter().filter(|e| !e.is_buy && e.first_sell).count() as u64,
            creator_buy_volume: creator_buy,
            creator_sell_volume: creator_sell,
            trade_size_min: min,
            trade_size_max: max,
            trade_size_median: median_raw(&mut sizes),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RollingWindows {
    windows: Vec<Window>,
    enabled: bool,
}

impl RollingWindows {
    pub fn new() -> Self {
        Self {
            windows: WINDOW_MS.iter().map(|d| Window::new(*d)).collect(),
            enabled: true,
        }
    }

    pub fn disabled() -> Self {
        let mut w = Self::new();
        w.enabled = false;
        w
    }

    pub fn push(&mut self, now_ms: i64, tick: TradeTick) {
        if !self.enabled {
            return;
        }
        for w in &mut self.windows {
            w.expire(now_ms);
            w.push(tick.clone());
        }
    }

    pub fn expire_all(&mut self, now_ms: i64) {
        if !self.enabled {
            return;
        }
        for w in &mut self.windows {
            w.expire(now_ms);
        }
    }

    pub fn snapshots(&self) -> Vec<RollingWindowSnapshot> {
        if !self.enabled {
            return WINDOW_MS
                .iter()
                .map(|d| RollingWindowSnapshot::empty(*d))
                .collect();
        }
        self.windows.iter().map(|w| w.snapshot()).collect()
    }

    pub fn snapshots_as_of(&self, at_ms: i64) -> Vec<RollingWindowSnapshot> {
        if !self.enabled {
            return WINDOW_MS
                .iter()
                .map(|d| RollingWindowSnapshot::empty(*d))
                .collect();
        }
        self.windows
            .iter()
            .map(|w| w.snapshot_as_of(at_ms))
            .collect()
    }

    pub fn by_ms(&self, duration_ms: i64) -> RollingWindowSnapshot {
        self.windows
            .iter()
            .find(|w| w.duration_ms == duration_ms)
            .map(|w| w.snapshot())
            .unwrap_or_else(|| RollingWindowSnapshot::empty(duration_ms))
    }

    pub fn drop_ticks(&mut self) {
        self.enabled = false;
        self.windows.clear();
    }
}

impl Default for RollingWindows {
    fn default() -> Self {
        Self::new()
    }
}
