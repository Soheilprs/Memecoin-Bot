use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Utc};

use crate::domain::raw_event::normalize_address;
use crate::domain::{
    CanonicalEvent, CanonicalStatus, Chain, Finality, Launchpad, LifecycleObserved, LifecycleType,
    QualityStatus, TokenDiscovered, TradeObserved, TradeSide,
};
use crate::metrics::DiscoveryMetrics;

use super::amt::{add_raw, net_signed, ratio_bps};
use super::clock::{EngineClock, StateClock, StateTime};
use super::lifecycle::TokenLifecycleState;
use super::market::{
    BondingCurveState, ConstantProductState, MarketState, MarketStateQuality, UniswapV4State,
};
use super::order::StateOrder;
use super::rolling::{RollingWindowSnapshot, RollingWindows, TradeTick};
use super::schedule::{MemoryPolicy, MemoryTier, SnapshotSchedule};
use super::snapshot::{SnapshotKind, TokenStateSnapshot, WalletSnapshot};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TokenKey {
    pub chain: Chain,
    pub token: String,
}

impl TokenKey {
    pub fn new(chain: Chain, token: impl Into<String>) -> Self {
        let raw = token.into();
        let token = match chain {
            Chain::Solana => raw,
            Chain::Base | Chain::Robinhood => normalize_address(&raw),
        };
        Self { chain, token }
    }
}

#[derive(Debug, Clone)]
pub struct TokenState {
    pub key: TokenKey,
    pub launchpad: Launchpad,
    pub creator: String,
    pub discovered_at: StateTime,
    pub first_trade_at: Option<StateTime>,
    pub last_trade_at: Option<StateTime>,
    pub lifecycle_state: TokenLifecycleState,
    pub curve: Option<String>,
    pub pool: Option<String>,
    pub quote_asset: Option<String>,
    pub quote_decimals: u8,
    pub token_decimals: u8,
    pub buy_count_total: u64,
    pub sell_count_total: u64,
    pub unique_buyers: HashSet<String>,
    pub unique_sellers: HashSet<String>,
    pub buyer_trade_counts: HashMap<String, u32>,
    pub seller_trade_counts: HashMap<String, u32>,
    pub trader_quote_volume: HashMap<String, String>,
    pub last_buy_at: Option<StateTime>,
    pub last_sell_at: Option<StateTime>,
    pub creator_last_sell_at: Option<StateTime>,
    pub buy_quote_volume_raw_total: String,
    pub sell_quote_volume_raw_total: String,
    pub buy_token_volume_raw_total: String,
    pub sell_token_volume_raw_total: String,
    pub creator_buy_count: u64,
    pub creator_sell_count: u64,
    pub creator_buy_quote_raw: String,
    pub creator_sell_quote_raw: String,
    pub last_trade_side: Option<TradeSide>,
    pub last_trade_token_raw: Option<String>,
    pub last_trade_quote_raw: Option<String>,
    pub market_state: MarketState,
    pub last_event_order: Option<StateOrder>,
    pub last_event_time: Option<StateTime>,
    pub last_event_id: Option<String>,
    pub last_block: Option<i64>,
    pub last_slot: Option<i64>,
    pub canonical_status: CanonicalStatus,
    pub data_quality: QualityStatus,
    pub snapshot_version: i32,
    pub last_emitted_snapshot_ms: i64,
    pub emitted_milestones: HashSet<i64>,
    pub launch_swept_at: Option<StateTime>,
    pub launch_swept_block: Option<u64>,
    pub pool_graduated_at: Option<StateTime>,
    pub pool_graduated_block: Option<u64>,
    pub graduation_gap_ms: Option<i64>,
    pub graduation_threshold_raw: Option<String>,
    pub snipe_tax_events_total: u64,
    pub latest_snipe_tax_amount: Option<String>,
    pub baseline_virtual_token: Option<String>,
    pub curve_progress_bps: Option<u32>,
    pub graduation_progress_bps: Option<u32>,
    pub rolling: RollingWindows,
    /// Point-in-time reconstruction for overdue live milestones. Bounded to hot window.
    pub trade_log: Vec<TradeTick>,
    pub memory_tier: MemoryTier,
    pub late_events: u64,
    pub first_amm_trade: bool,
}

impl TokenState {
    pub fn age_ms(&self, now: StateTime) -> i64 {
        now.unix_ms.saturating_sub(self.discovered_at.unix_ms)
    }

    pub fn unique_buyers_total(&self) -> u64 {
        self.unique_buyers.len() as u64
    }

    pub fn unique_sellers_total(&self) -> u64 {
        self.unique_sellers.len() as u64
    }

    pub fn creator_net_quote_flow(&self) -> String {
        net_signed(&self.creator_buy_quote_raw, &self.creator_sell_quote_raw)
    }
}

pub struct StateEngine {
    pub clock: EngineClock,
    pub schedule: SnapshotSchedule,
    pub memory: MemoryPolicy,
    pub source_quality: QualityStatus,
    pub source_session_id: Option<i64>,
    tokens: HashMap<TokenKey, TokenState>,
    by_curve: HashMap<(Chain, String), TokenKey>,
    by_pool: HashMap<(Chain, String), TokenKey>,
    reorder: BTreeMap<StateOrder, CanonicalEvent>,
    last_applied: Option<StateOrder>,
    pub late_events: u64,
    pub rebuilds: u64,
    pub evictions: u64,
    pending_rebuilds: HashSet<TokenKey>,
    snapshot_buffer: Vec<TokenStateSnapshot>,
    pub history: Vec<TokenStateSnapshot>,
    unresolved_trades: Vec<(StateOrder, TradeObserved, StateTime)>,
}

impl StateEngine {
    pub fn replay(quality: QualityStatus, session_id: Option<i64>) -> Self {
        Self::new(EngineClock::replay(), quality, session_id)
    }

    pub fn live(quality: QualityStatus, session_id: Option<i64>) -> Self {
        Self::new(EngineClock::live(), quality, session_id)
    }

    pub fn new(clock: EngineClock, quality: QualityStatus, session_id: Option<i64>) -> Self {
        Self {
            clock,
            schedule: SnapshotSchedule::default_research(),
            memory: MemoryPolicy::default_research(),
            source_quality: quality,
            source_session_id: session_id,
            tokens: HashMap::new(),
            by_curve: HashMap::new(),
            by_pool: HashMap::new(),
            reorder: BTreeMap::new(),
            last_applied: None,
            late_events: 0,
            rebuilds: 0,
            evictions: 0,
            pending_rebuilds: HashSet::new(),
            snapshot_buffer: Vec::new(),
            history: Vec::new(),
            unresolved_trades: Vec::new(),
        }
    }

    pub fn active_count(&self) -> usize {
        self.tokens.len()
    }

    pub fn get(&self, chain: Chain, token: &str) -> Option<&TokenState> {
        self.tokens.get(&TokenKey::new(chain, token))
    }

    pub fn take_snapshots(&mut self) -> Vec<TokenStateSnapshot> {
        std::mem::take(&mut self.snapshot_buffer)
    }

    pub fn pending_rebuilds(&mut self) -> Vec<TokenKey> {
        self.pending_rebuilds.drain().collect()
    }

    /// Apply without draining the snapshot buffer. Live collect leaves snapshots
    /// for `tick_live` so FeatureEngine runs on the same path as timer milestones.
    pub fn push_event(&mut self, event: CanonicalEvent) {
        let order = StateOrder::from_canonical(&event, 0);
        if let Some(last) = &self.last_applied {
            if order < *last {
                if self.clock.is_replay() {
                    self.late_events += 1;
                    DiscoveryMetrics::late_event(order.chain);
                    if let Some(key) = token_key_of(&event) {
                        self.pending_rebuilds.insert(key);
                    }
                    return;
                }
                self.reorder.insert(order, event);
                if self.reorder.len() > 64 {
                    self.flush_reorder();
                }
                return;
            }
        }
        self.apply_in_order(event, order);
        self.flush_reorder();
    }

    pub fn apply(&mut self, event: CanonicalEvent) -> Vec<TokenStateSnapshot> {
        self.push_event(event);
        self.take_snapshots()
    }

    pub fn discard_snapshot_buffer(&mut self) {
        self.snapshot_buffer.clear();
    }

    pub fn mark_already_emitted(&mut self, key: &TokenKey, snaps: &[TokenStateSnapshot]) {
        let Some(st) = self.tokens.get_mut(key) else {
            return;
        };
        st.emitted_milestones.clear();
        st.last_emitted_snapshot_ms = st.discovered_at.unix_ms;
        for s in snaps {
            if s.snapshot_kind == SnapshotKind::Milestone {
                st.emitted_milestones.insert(s.age_ms);
            }
            let ms = s.snapshot_time.timestamp_millis();
            if ms > st.last_emitted_snapshot_ms {
                st.last_emitted_snapshot_ms = ms;
            }
        }
    }

    pub fn apply_sorted(&mut self, mut events: Vec<CanonicalEvent>) -> Vec<TokenStateSnapshot> {
        events.sort_by_key(|e| StateOrder::from_canonical(e, 0));
        let mut out = Vec::new();
        for e in events {
            out.extend(self.apply(e));
        }
        out
    }

    /// Advance replay clock and emit remaining milestone/periodic snapshots (dead tokens included).
    pub fn finish_until(&mut self, until: StateTime) -> Vec<TokenStateSnapshot> {
        self.clock.advance_to(until);
        let keys: Vec<TokenKey> = self.tokens.keys().cloned().collect();
        for key in keys {
            self.emit_due_snapshots(key.clone(), until);
            self.maybe_evict(key, until);
        }
        self.take_snapshots()
    }

    pub fn finish_all_milestones(&mut self) -> Vec<TokenStateSnapshot> {
        let last = self.schedule.last_milestone_ms();
        let mut until = self.clock.now();
        for st in self.tokens.values() {
            let t = st.discovered_at.saturating_add_ms(last);
            if t > until {
                until = t;
            }
        }
        self.finish_until(until)
    }

    pub fn watched_keys(&self) -> Vec<TokenKey> {
        self.tokens.keys().cloned().collect()
    }

    pub fn token_discovered_at(&self, key: &TokenKey) -> Option<StateTime> {
        self.tokens.get(key).map(|s| s.discovered_at)
    }

    pub fn emitted_milestones(&self, key: &TokenKey) -> HashSet<i64> {
        self.tokens
            .get(key)
            .map(|s| s.emitted_milestones.clone())
            .unwrap_or_default()
    }

    pub fn tick_live(&mut self) -> Vec<TokenStateSnapshot> {
        let now = self.clock.now();
        self.finish_until(now)
    }

    pub fn rebuild_token(
        &mut self,
        key: TokenKey,
        events: Vec<CanonicalEvent>,
    ) -> Vec<TokenStateSnapshot> {
        self.rebuilds += 1;
        DiscoveryMetrics::state_rebuild(key.chain);
        self.tokens.remove(&key);
        self.by_curve.retain(|_, v| *v != key);
        self.by_pool.retain(|_, v| *v != key);
        self.last_applied = None;
        self.reorder.clear();
        let mut sorted = events;
        sorted.sort_by_key(|e| StateOrder::from_canonical(e, 0));
        let mut out = Vec::new();
        for e in sorted {
            out.extend(self.apply(e));
        }
        if let Some(st) = self.tokens.get_mut(&key) {
            st.snapshot_version = st.snapshot_version.saturating_add(1);
        }
        for s in &mut out {
            if TokenKey::new(s.chain, &s.token_address) == key {
                if let Some(st) = self.tokens.get(&key) {
                    s.version = st.snapshot_version;
                    s.fingerprint = s.compute_fingerprint();
                }
            }
        }
        out
    }

    pub fn note_orphan(&mut self, chain: Chain, token: &str) {
        self.pending_rebuilds.insert(TokenKey::new(chain, token));
        self.late_events += 1;
    }

    fn flush_reorder(&mut self) {
        while let Some(entry) = self.reorder.keys().next().cloned() {
            if let Some(last) = &self.last_applied {
                if entry < *last {
                    if let Some(ev) = self.reorder.remove(&entry) {
                        self.late_events += 1;
                        DiscoveryMetrics::late_event(entry.chain);
                        if let Some(key) = token_key_of(&ev) {
                            self.pending_rebuilds.insert(key);
                        }
                    }
                    continue;
                }
            }
            if let Some(ev) = self.reorder.remove(&entry) {
                self.apply_in_order(ev, entry);
            }
        }
    }

    fn apply_in_order(&mut self, event: CanonicalEvent, order: StateOrder) {
        let t = event_time(&event);
        self.clock.advance_to(t);
        if let Some(key) = token_key_of(&event) {
            self.emit_due_snapshots(key, t);
        } else if let CanonicalEvent::Trade(tr) = &event {
            if let Some(key) = self.resolve_trade(tr) {
                self.emit_due_snapshots(key, t);
            }
        } else if let CanonicalEvent::Lifecycle(life) = &event {
            if let Some(key) = self.resolve_life(life) {
                self.emit_due_snapshots(key, t);
            }
        }
        match event {
            CanonicalEvent::TokenDiscovered(tok) => self.apply_discovered(*tok, order.clone(), t),
            CanonicalEvent::Trade(tr) => self.apply_trade(*tr, order.clone(), t),
            CanonicalEvent::Lifecycle(life) => self.apply_lifecycle(*life, order.clone(), t),
        }
        DiscoveryMetrics::state_event_processed(order.chain);
        self.last_applied = Some(order);
    }

    fn apply_discovered(&mut self, tok: TokenDiscovered, order: StateOrder, t: StateTime) {
        if tok.token_address.is_empty() {
            return;
        }
        let key = TokenKey::new(tok.chain, &tok.token_address);
        let inserted = !self.tokens.contains_key(&key);
        let initial = TokenLifecycleState::initial(tok.launchpad);
        let discovered_at = tok
            .chain_timestamp
            .map(StateTime::from_datetime)
            .unwrap_or(t);
        let state = self
            .tokens
            .entry(key.clone())
            .or_insert_with(|| TokenState {
                key: key.clone(),
                launchpad: tok.launchpad,
                creator: tok.creator.clone(),
                discovered_at,
                first_trade_at: None,
                last_trade_at: None,
                lifecycle_state: initial,
                curve: tok.curve.clone(),
                pool: tok.pool.clone(),
                quote_asset: tok.quote_asset.clone(),
                quote_decimals: 9,
                token_decimals: 6,
                buy_count_total: 0,
                sell_count_total: 0,
                unique_buyers: HashSet::new(),
                unique_sellers: HashSet::new(),
                buyer_trade_counts: HashMap::new(),
                seller_trade_counts: HashMap::new(),
                trader_quote_volume: HashMap::new(),
                last_buy_at: None,
                last_sell_at: None,
                creator_last_sell_at: None,
                buy_quote_volume_raw_total: "0".into(),
                sell_quote_volume_raw_total: "0".into(),
                buy_token_volume_raw_total: "0".into(),
                sell_token_volume_raw_total: "0".into(),
                creator_buy_count: 0,
                creator_sell_count: 0,
                creator_buy_quote_raw: "0".into(),
                creator_sell_quote_raw: "0".into(),
                last_trade_side: None,
                last_trade_token_raw: None,
                last_trade_quote_raw: None,
                market_state: match tok.launchpad {
                    Launchpad::PumpFun | Launchpad::PonsV2 => {
                        MarketState::BondingCurve(BondingCurveState::default())
                    }
                    Launchpad::ClankerV4 => MarketState::UniswapV4(UniswapV4State {
                        pool_id: tok.pool.clone(),
                        quote_asset: tok.quote_asset.clone(),
                        ..Default::default()
                    }),
                    Launchpad::PumpSwap => MarketState::ConstantProduct(ConstantProductState {
                        pool: tok.pool.clone(),
                        token: Some(tok.token_address.clone()),
                        quote_asset: tok.quote_asset.clone(),
                        quality: MarketStateQuality::Partial,
                        ..Default::default()
                    }),
                    Launchpad::Unknown => MarketState::Unknown,
                },
                last_event_order: Some(order.clone()),
                last_event_time: Some(t),
                last_event_id: Some(tok.raw_event_id.clone()),
                last_block: tok.block_number.map(|v| v as i64),
                last_slot: tok.slot.map(|v| v as i64),
                canonical_status: CanonicalStatus::Canonical,
                data_quality: self.source_quality,
                snapshot_version: 1,
                last_emitted_snapshot_ms: discovered_at.unix_ms,
                emitted_milestones: HashSet::new(),
                launch_swept_at: None,
                launch_swept_block: None,
                pool_graduated_at: None,
                pool_graduated_block: None,
                graduation_gap_ms: None,
                graduation_threshold_raw: None,
                snipe_tax_events_total: 0,
                latest_snipe_tax_amount: None,
                baseline_virtual_token: None,
                curve_progress_bps: None,
                graduation_progress_bps: None,
                rolling: RollingWindows::new(),
                trade_log: Vec::new(),
                memory_tier: MemoryTier::Hot,
                late_events: 0,
                first_amm_trade: false,
            });
        if state.creator.is_empty() {
            state.creator = tok.creator.clone();
        }
        if state.curve.is_none() {
            state.curve = tok.curve.clone();
        }
        if state.pool.is_none() {
            state.pool = tok.pool.clone();
        }
        if state.quote_asset.is_none() {
            state.quote_asset = tok.quote_asset.clone();
        }
        state.last_event_order = Some(order.clone());
        state.last_event_time = Some(t);
        state.last_event_id = Some(tok.raw_event_id.clone());
        state.last_block = tok.block_number.map(|v| v as i64);
        state.last_slot = tok.slot.map(|v| v as i64);
        if tok.launchpad == Launchpad::ClankerV4 {
            state.lifecycle_state = TokenLifecycleState::AmmActive;
            state.token_decimals = 18;
            state.quote_decimals = 18;
        }
        if tok.launchpad == Launchpad::PonsV2 {
            state.token_decimals = 18;
            state.quote_decimals = 18;
        }
        let curve = state.curve.clone();
        let pool = state.pool.clone();
        if let Some(c) = curve {
            self.by_curve
                .insert((key.chain, addr_key(key.chain, &c)), key.clone());
        }
        if let Some(p) = pool {
            self.by_pool
                .insert((key.chain, addr_key(key.chain, &p)), key.clone());
        }
        if inserted {
            self.emit_lifecycle_snapshot(key, "TOKEN_CREATED");
        }
        self.flush_unresolved_trades();
        DiscoveryMetrics::token_states_active(self.tokens.len());
    }

    fn apply_trade(&mut self, trade: TradeObserved, order: StateOrder, t: StateTime) {
        let Some(key) = self.resolve_trade(&trade).or_else(|| {
            if trade.token_address.is_empty() {
                None
            } else {
                Some(TokenKey::new(trade.chain, &trade.token_address))
            }
        }) else {
            self.unresolved_trades.push((order, trade, t));
            return;
        };
        if !self.tokens.contains_key(&key) {
            let mut tok = TokenDiscovered {
                chain: trade.chain,
                chain_id: None,
                token_address: key.token.clone(),
                creator: String::new(),
                launchpad: trade.launchpad,
                factory_or_program: String::new(),
                pool: trade.pool.clone(),
                curve: trade.curve.clone(),
                quote_asset: Some(trade.quote_asset.clone()),
                launch_mechanism: crate::domain::LaunchMechanism::Unknown,
                bonding_curve: trade.curve.is_some(),
                graduation_model: crate::domain::GraduationModel::Unknown,
                block_number: trade.block_number,
                block_hash: trade.block_hash.clone(),
                slot: trade.slot,
                tx_hash_or_signature: trade.tx_hash_or_signature.clone(),
                instruction_index: trade.instruction_index,
                inner_instruction_index: trade.inner_instruction_index,
                log_index: trade.log_index,
                chain_timestamp: trade.chain_timestamp,
                observed_at: trade.observed_at,
                persisted_at: None,
                source: trade.source.clone(),
                decoder_version: trade.decoder_version.clone(),
                initial_liquidity: None,
                raw_event_id: trade.raw_event_id.clone(),
            };
            if tok.token_address.is_empty() {
                tok.token_address = key.token.clone();
            }
            self.apply_discovered(tok, order.clone(), t);
        }
        let is_first_trade;
        let is_first_amm;
        let first_buy;
        let first_sell;
        {
            let Some(st) = self.tokens.get_mut(&key) else {
                return;
            };
            is_first_trade = st.first_trade_at.is_none();
            if is_first_trade {
                st.first_trade_at = Some(t);
                if st.lifecycle_state == TokenLifecycleState::Discovered
                    && matches!(st.launchpad, Launchpad::PumpFun | Launchpad::PonsV2)
                {
                    st.lifecycle_state = TokenLifecycleState::CurveActive;
                }
            }
            st.last_trade_at = Some(t);
            match trade.side {
                TradeSide::Buy => st.last_buy_at = Some(t),
                TradeSide::Sell => {
                    st.last_sell_at = Some(t);
                    if !st.creator.is_empty() && st.creator == trade.trader {
                        st.creator_last_sell_at = Some(t);
                    }
                }
            }
            st.quote_asset = Some(trade.quote_asset.clone());
            st.token_decimals = trade.base_decimals;
            st.quote_decimals = trade.quote_decimals;
            let is_creator = !st.creator.is_empty() && st.creator == trade.trader;
            first_buy = trade.side == TradeSide::Buy && !st.unique_buyers.contains(&trade.trader);
            first_sell =
                trade.side == TradeSide::Sell && !st.unique_sellers.contains(&trade.trader);
            match trade.side {
                TradeSide::Buy => {
                    st.buy_count_total += 1;
                    st.unique_buyers.insert(trade.trader.clone());
                    *st.buyer_trade_counts
                        .entry(trade.trader.clone())
                        .or_insert(0) += 1;
                    st.buy_quote_volume_raw_total =
                        add_raw(&st.buy_quote_volume_raw_total, &trade.quote_amount_raw);
                    st.buy_token_volume_raw_total =
                        add_raw(&st.buy_token_volume_raw_total, &trade.base_amount_raw);
                    if is_creator {
                        st.creator_buy_count += 1;
                        st.creator_buy_quote_raw =
                            add_raw(&st.creator_buy_quote_raw, &trade.quote_amount_raw);
                    }
                }
                TradeSide::Sell => {
                    st.sell_count_total += 1;
                    st.unique_sellers.insert(trade.trader.clone());
                    *st.seller_trade_counts
                        .entry(trade.trader.clone())
                        .or_insert(0) += 1;
                    st.sell_quote_volume_raw_total =
                        add_raw(&st.sell_quote_volume_raw_total, &trade.quote_amount_raw);
                    st.sell_token_volume_raw_total =
                        add_raw(&st.sell_token_volume_raw_total, &trade.base_amount_raw);
                    if is_creator {
                        st.creator_sell_count += 1;
                        st.creator_sell_quote_raw =
                            add_raw(&st.creator_sell_quote_raw, &trade.quote_amount_raw);
                    }
                }
            }
            let prev_vol = st
                .trader_quote_volume
                .get(&trade.trader)
                .cloned()
                .unwrap_or_else(|| "0".into());
            st.trader_quote_volume.insert(
                trade.trader.clone(),
                add_raw(&prev_vol, &trade.quote_amount_raw),
            );
            st.last_trade_side = Some(trade.side);
            st.last_trade_token_raw = Some(trade.base_amount_raw.clone());
            st.last_trade_quote_raw = Some(trade.quote_amount_raw.clone());
            st.last_event_order = Some(order.clone());
            st.last_event_time = Some(t);
            st.last_event_id = Some(trade.event_id.clone());
            st.last_block = trade.block_number.map(|v| v as i64);
            st.last_slot = trade.slot.map(|v| v as i64);
            update_market_from_trade(st, &trade);
            if st.lifecycle_state == TokenLifecycleState::AmmActive && !st.first_amm_trade {
                st.first_amm_trade = true;
                is_first_amm = true;
            } else {
                is_first_amm = false;
            }
            let tick = TradeTick {
                time_ms: t.unix_ms,
                trader: trade.trader.clone(),
                is_buy: trade.side == TradeSide::Buy,
                is_creator,
                quote_raw: trade.quote_amount_raw.clone(),
                token_raw: trade.base_amount_raw.clone(),
                first_buy,
                first_sell,
            };
            st.trade_log.push(tick.clone());
            if st.trade_log.len() > 8_192 {
                st.trade_log.drain(0..st.trade_log.len() - 8_192);
            }
            if st.memory_tier == MemoryTier::Hot {
                st.rolling.push(t.unix_ms, tick);
            }
            DiscoveryMetrics::rolling_window_update(st.key.chain);
        }
        if is_first_trade {
            self.emit_lifecycle_snapshot(key.clone(), "FIRST_TRADE");
        }
        if is_first_amm {
            self.emit_lifecycle_snapshot(key, "AMM_FIRST_TRADE");
        }
    }

    fn apply_lifecycle(&mut self, life: LifecycleObserved, order: StateOrder, t: StateTime) {
        let Some(key) = self.resolve_life(&life).or_else(|| {
            if life.token_address.is_empty() {
                None
            } else {
                Some(TokenKey::new(life.chain, &life.token_address))
            }
        }) else {
            return;
        };
        if !self.tokens.contains_key(&key) && !life.token_address.is_empty() {
            let tok = TokenDiscovered {
                chain: life.chain,
                chain_id: None,
                token_address: key.token.clone(),
                creator: String::new(),
                launchpad: life.launchpad,
                factory_or_program: life.factory.clone().unwrap_or_default(),
                pool: life.pool.clone(),
                curve: life.curve.clone(),
                quote_asset: None,
                launch_mechanism: crate::domain::LaunchMechanism::Unknown,
                bonding_curve: life.curve.is_some(),
                graduation_model: crate::domain::GraduationModel::Unknown,
                block_number: life.block_number,
                block_hash: life.block_hash.clone(),
                slot: life.slot,
                tx_hash_or_signature: life.tx_hash_or_signature.clone(),
                instruction_index: life.instruction_index,
                inner_instruction_index: life.inner_instruction_index,
                log_index: life.log_index,
                chain_timestamp: life.chain_timestamp,
                observed_at: life.observed_at,
                persisted_at: None,
                source: life.source.clone(),
                decoder_version: life.decoder_version.clone(),
                initial_liquidity: None,
                raw_event_id: life.raw_event_id.clone(),
            };
            self.apply_discovered(tok, order.clone(), t);
        }
        let trigger;
        {
            let Some(st) = self.tokens.get_mut(&key) else {
                return;
            };
            if let Some(c) = &life.curve {
                st.curve = Some(c.clone());
            }
            if let Some(p) = &life.pool {
                st.pool = Some(p.clone());
            }
            match life.lifecycle_type {
                LifecycleType::TokenCreated => {
                    trigger = "TOKEN_CREATED";
                    if st.launchpad == Launchpad::ClankerV4 {
                        st.lifecycle_state = TokenLifecycleState::AmmActive;
                    }
                    if let Some(th) = life
                        .metadata
                        .get("graduationThreshold")
                        .and_then(|v| v.as_str())
                    {
                        st.graduation_threshold_raw = Some(th.to_string());
                    }
                }
                LifecycleType::MigrationStarted => {
                    trigger = "MIGRATION_STARTED";
                    st.lifecycle_state = TokenLifecycleState::Migrating;
                }
                LifecycleType::Migrated | LifecycleType::CurveCompleted => {
                    trigger = if life.lifecycle_type == LifecycleType::Migrated {
                        "MIGRATED"
                    } else {
                        "CURVE_COMPLETED"
                    };
                    st.lifecycle_state = TokenLifecycleState::Migrating;
                    if let Some(pool) = &life.pool {
                        st.pool = Some(pool.clone());
                        st.market_state = MarketState::ConstantProduct(ConstantProductState {
                            pool: Some(pool.clone()),
                            token: Some(st.key.token.clone()),
                            quote_asset: st.quote_asset.clone(),
                            quality: MarketStateQuality::Partial,
                            ..Default::default()
                        });
                    }
                }
                LifecycleType::LaunchSwept => {
                    trigger = "LAUNCH_SWEPT";
                    st.lifecycle_state = TokenLifecycleState::GraduationGap;
                    st.launch_swept_at = Some(t);
                    st.launch_swept_block = life.block_number;
                }
                LifecycleType::PoolCreated => {
                    trigger = "POOL_CREATED";
                    if st.launchpad == Launchpad::PumpFun || st.launchpad == Launchpad::PumpSwap {
                        st.lifecycle_state = TokenLifecycleState::AmmActive;
                        st.launchpad = Launchpad::PumpSwap;
                    }
                    if let Some(pool) = &life.pool {
                        st.pool = Some(pool.clone());
                        if !matches!(st.market_state, MarketState::UniswapV4(_)) {
                            st.market_state = MarketState::ConstantProduct(ConstantProductState {
                                pool: Some(pool.clone()),
                                token: Some(st.key.token.clone()),
                                quote_asset: st.quote_asset.clone(),
                                quality: MarketStateQuality::Partial,
                                ..Default::default()
                            });
                        }
                    }
                    apply_v4_meta(st, &life.metadata);
                }
                LifecycleType::PoolGraduated => {
                    trigger = "POOL_GRADUATED";
                    st.lifecycle_state = TokenLifecycleState::AmmActive;
                    st.pool_graduated_at = Some(t);
                    st.pool_graduated_block = life.block_number;
                    if let (Some(a), Some(b)) = (st.launch_swept_at, st.pool_graduated_at) {
                        st.graduation_gap_ms = Some(b.unix_ms.saturating_sub(a.unix_ms));
                    }
                    apply_v4_meta(st, &life.metadata);
                }
                LifecycleType::SnipeTaxCharged => {
                    trigger = "SNIPE_TAX_CHARGED";
                    st.snipe_tax_events_total += 1;
                    st.latest_snipe_tax_amount = life
                        .metadata
                        .get("amount")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
            }
            st.last_event_order = Some(order);
            st.last_event_time = Some(t);
            st.last_event_id = Some(life.event_id.clone());
            st.last_block = life.block_number.map(|v| v as i64);
            st.last_slot = life.slot.map(|v| v as i64);
            if let Some(th) = &st.graduation_threshold_raw {
                st.graduation_progress_bps = ratio_bps(&st.buy_quote_volume_raw_total, th);
            }
            let curve = st.curve.clone();
            let pool = st.pool.clone();
            let chain = st.key.chain;
            let _ = st;
            if let Some(c) = curve {
                self.by_curve
                    .insert((chain, addr_key(chain, &c)), key.clone());
            }
            if let Some(p) = pool {
                self.by_pool
                    .insert((chain, addr_key(chain, &p)), key.clone());
            }
        }
        if trigger != "SNIPE_TAX_CHARGED" {
            self.emit_lifecycle_snapshot(key, trigger);
        }
        self.flush_unresolved_trades();
    }

    fn flush_unresolved_trades(&mut self) {
        let pending = std::mem::take(&mut self.unresolved_trades);
        for (order, trade, t) in pending {
            if self.resolve_trade(&trade).is_some() || !trade.token_address.is_empty() {
                self.apply_trade(trade, order, t);
            } else {
                self.unresolved_trades.push((order, trade, t));
            }
        }
    }

    fn resolve_trade(&self, trade: &TradeObserved) -> Option<TokenKey> {
        if !trade.token_address.is_empty() {
            return Some(TokenKey::new(trade.chain, &trade.token_address));
        }
        if let Some(c) = &trade.curve {
            if let Some(k) = self.by_curve.get(&(trade.chain, addr_key(trade.chain, c))) {
                return Some(k.clone());
            }
        }
        if let Some(p) = &trade.pool {
            if let Some(k) = self.by_pool.get(&(trade.chain, addr_key(trade.chain, p))) {
                return Some(k.clone());
            }
        }
        None
    }

    fn resolve_life(&self, life: &LifecycleObserved) -> Option<TokenKey> {
        if !life.token_address.is_empty() {
            return Some(TokenKey::new(life.chain, &life.token_address));
        }
        if let Some(c) = &life.curve {
            if let Some(k) = self.by_curve.get(&(life.chain, addr_key(life.chain, c))) {
                return Some(k.clone());
            }
        }
        if let Some(p) = &life.pool {
            if let Some(k) = self.by_pool.get(&(life.chain, addr_key(life.chain, p))) {
                return Some(k.clone());
            }
        }
        None
    }

    fn emit_due_snapshots(&mut self, key: TokenKey, until: StateTime) {
        let (discovered, from, already) = {
            let Some(st) = self.tokens.get(&key) else {
                return;
            };
            (
                st.discovered_at.unix_ms,
                st.last_emitted_snapshot_ms,
                st.emitted_milestones.clone(),
            )
        };
        let due = self.schedule.due_times(discovered, from, until.unix_ms);
        let milestones: HashSet<i64> = self.schedule.milestones_ms.iter().copied().collect();
        for t in due {
            let age = t.saturating_sub(discovered);
            let kind = if milestones.contains(&age) && !already.contains(&age) {
                SnapshotKind::Milestone
            } else {
                SnapshotKind::Periodic
            };
            if kind == SnapshotKind::Milestone || kind == SnapshotKind::Periodic {
                self.push_snapshot(key.clone(), StateTime { unix_ms: t }, kind, None);
            }
        }
    }

    fn emit_lifecycle_snapshot(&mut self, key: TokenKey, trigger: &str) {
        let now = self.clock.now();
        self.push_snapshot(key, now, SnapshotKind::Lifecycle, Some(trigger));
    }

    fn push_snapshot(
        &mut self,
        key: TokenKey,
        at: StateTime,
        kind: SnapshotKind,
        trigger: Option<&str>,
    ) {
        let Some(st) = self.tokens.get_mut(&key) else {
            return;
        };
        if kind != SnapshotKind::Lifecycle && at.unix_ms <= st.last_emitted_snapshot_ms {
            return;
        }
        let rolls = st.rolling.snapshots_as_of(at.unix_ms);
        let (buy_n, sell_n, ub, us) = counts_as_of(st, at.unix_ms);
        let pick = |ms: i64| {
            rolls
                .iter()
                .find(|r| r.duration_ms == ms)
                .cloned()
                .unwrap_or_else(|| RollingWindowSnapshot {
                    duration_ms: ms,
                    buy_quote_volume_raw: "0".into(),
                    sell_quote_volume_raw: "0".into(),
                    buy_token_volume_raw: "0".into(),
                    sell_token_volume_raw: "0".into(),
                    net_quote_flow: "0".into(),
                    creator_buy_volume: "0".into(),
                    creator_sell_volume: "0".into(),
                    ..Default::default()
                })
        };
        let mut snap = TokenStateSnapshot {
            id: None,
            chain: key.chain,
            token_address: key.token.clone(),
            launchpad: st.launchpad,
            snapshot_time: at.datetime(),
            age_ms: at.unix_ms.saturating_sub(st.discovered_at.unix_ms),
            snapshot_kind: kind,
            lifecycle_trigger: trigger.map(|s| s.to_string()),
            lifecycle_state: st.lifecycle_state,
            quote_asset: st.quote_asset.clone(),
            buy_count_total: buy_n,
            sell_count_total: sell_n,
            unique_buyers_total: ub,
            unique_sellers_total: us,
            buy_quote_volume_raw_total: st.buy_quote_volume_raw_total.clone(),
            sell_quote_volume_raw_total: st.sell_quote_volume_raw_total.clone(),
            buy_token_volume_raw_total: st.buy_token_volume_raw_total.clone(),
            sell_token_volume_raw_total: st.sell_token_volume_raw_total.clone(),
            creator_buy_count: st.creator_buy_count,
            creator_sell_count: st.creator_sell_count,
            creator_buy_quote_raw: st.creator_buy_quote_raw.clone(),
            creator_sell_quote_raw: st.creator_sell_quote_raw.clone(),
            last_trade_side: st.last_trade_side,
            last_trade_token_raw: st.last_trade_token_raw.clone(),
            last_trade_quote_raw: st.last_trade_quote_raw.clone(),
            last_trade_token_decimals: Some(st.token_decimals),
            last_trade_quote_decimals: Some(st.quote_decimals),
            curve_progress_bps: st.curve_progress_bps,
            graduation_progress_bps: st.graduation_progress_bps,
            market_state_type: st.market_state.type_name().to_string(),
            market_state: st.market_state.clone(),
            rolling_5s: pick(5_000),
            rolling_15s: pick(15_000),
            rolling_30s: pick(30_000),
            rolling_60s: pick(60_000),
            rolling_120s: pick(120_000),
            rolling_300s: pick(300_000),
            rolling_900s: pick(900_000),
            as_of_event_id: st.last_event_id.clone(),
            as_of_block: st.last_block,
            as_of_slot: st.last_slot,
            as_of_event_order: st
                .last_event_order
                .as_ref()
                .map(|o| o.encoded())
                .unwrap_or_default(),
            data_quality: st.data_quality,
            source_session_id: self.source_session_id,
            canonical_status: st.canonical_status,
            finality: Finality::Confirmed,
            version: st.snapshot_version,
            superseded: false,
            fingerprint: String::new(),
            created_at: Utc::now(),
            wallet: wallet_snapshot_as_of(st, at),
        };
        snap.fingerprint = snap.compute_fingerprint();
        if kind != SnapshotKind::Lifecycle {
            st.last_emitted_snapshot_ms = at.unix_ms.max(st.last_emitted_snapshot_ms);
            let age = snap.age_ms;
            if self.schedule.milestones_ms.contains(&age) {
                st.emitted_milestones.insert(age);
            }
        }
        DiscoveryMetrics::snapshot_created(key.chain, st.launchpad, kind.as_str());
        self.history.push(snap.clone());
        self.snapshot_buffer.push(snap);
    }

    fn maybe_evict(&mut self, key: TokenKey, now: StateTime) {
        let Some(st) = self.tokens.get_mut(&key) else {
            return;
        };
        let age = st.age_ms(now);
        if age >= self.memory.cold_ms {
            self.tokens.remove(&key);
            self.evictions += 1;
            DiscoveryMetrics::token_state_evicted(key.chain);
            DiscoveryMetrics::token_states_active(self.tokens.len());
            return;
        }
        if age >= self.memory.hot_ms && st.memory_tier == MemoryTier::Hot {
            st.rolling.drop_ticks();
            st.memory_tier = MemoryTier::Warm;
        }
    }
}

fn counts_as_of(st: &TokenState, at_ms: i64) -> (u64, u64, u64, u64) {
    if st.trade_log.is_empty() {
        return (
            st.buy_count_total,
            st.sell_count_total,
            st.unique_buyers_total(),
            st.unique_sellers_total(),
        );
    }
    let mut buys = 0u64;
    let mut sells = 0u64;
    let mut ub = HashSet::new();
    let mut us = HashSet::new();
    for t in &st.trade_log {
        if t.time_ms > at_ms {
            continue;
        }
        if t.is_buy {
            buys += 1;
            ub.insert(t.trader.as_str());
        } else {
            sells += 1;
            us.insert(t.trader.as_str());
        }
    }
    (buys, sells, ub.len() as u64, us.len() as u64)
}

fn wallet_snapshot_as_of(st: &TokenState, at: StateTime) -> WalletSnapshot {
    wallet_snapshot(st, at)
}

fn wallet_snapshot(st: &TokenState, at: StateTime) -> WalletSnapshot {
    let unique_traders = st.unique_buyers.union(&st.unique_sellers).count() as u64;
    let repeat_buyer_count = st.buyer_trade_counts.values().filter(|c| **c > 1).count() as u64;
    let mean_buys_per_buyer_milli = if st.unique_buyers.is_empty() {
        Some(0)
    } else {
        Some(st.buy_count_total.saturating_mul(1000) / st.unique_buyers.len() as u64)
    };
    let mut buy_counts: Vec<u32> = st.buyer_trade_counts.values().copied().collect();
    buy_counts.sort_unstable();
    let median_buys_per_buyer = if buy_counts.is_empty() {
        Some(0)
    } else {
        Some(buy_counts[buy_counts.len() / 2] as u64)
    };
    let mut trade_counts: HashMap<String, u32> = st.buyer_trade_counts.clone();
    for (k, v) in &st.seller_trade_counts {
        *trade_counts.entry(k.clone()).or_insert(0) += *v;
    }
    let total_trades = st.buy_count_total.saturating_add(st.sell_count_total);
    let top_trades = trade_counts.values().copied().max().unwrap_or(0) as u64;
    let top_trader_trade_share_bps = ratio_bps_u64(top_trades, total_trades);
    let top_vol = st
        .trader_quote_volume
        .values()
        .max_by_key(|v| super::amt::parse_u256(v))
        .cloned();
    let tot_vol = add_raw(
        &st.buy_quote_volume_raw_total,
        &st.sell_quote_volume_raw_total,
    );
    let top_trader_volume_share_bps = top_vol.as_deref().and_then(|v| ratio_bps(v, &tot_vol));
    let age = |t: Option<StateTime>| t.map(|x| at.unix_ms.saturating_sub(x.unix_ms));
    WalletSnapshot {
        unique_traders_total: Some(unique_traders),
        repeat_buyer_count: Some(repeat_buyer_count),
        mean_buys_per_buyer_milli,
        median_buys_per_buyer,
        last_buy_age_ms: age(st.last_buy_at),
        last_sell_age_ms: age(st.last_sell_at),
        last_trade_age_ms: age(st.last_trade_at),
        creator_last_sell_age_ms: age(st.creator_last_sell_at),
        top_trader_trade_share_bps,
        top_trader_volume_share_bps,
    }
}

fn ratio_bps_u64(n: u64, d: u64) -> Option<u32> {
    if d == 0 {
        return None;
    }
    Some(((n as u128 * 10_000) / d as u128).min(u32::MAX as u128) as u32)
}

fn addr_key(chain: Chain, value: &str) -> String {
    match chain {
        Chain::Solana => value.to_string(),
        Chain::Base | Chain::Robinhood => normalize_address(value),
    }
}

fn event_time(ev: &CanonicalEvent) -> StateTime {
    let dt: Option<DateTime<Utc>> = match ev {
        CanonicalEvent::TokenDiscovered(t) => t.chain_timestamp.or(Some(t.observed_at)),
        CanonicalEvent::Trade(t) => t.chain_timestamp.or(Some(t.observed_at)),
        CanonicalEvent::Lifecycle(t) => t.chain_timestamp.or(Some(t.observed_at)),
    };
    dt.map(StateTime::from_datetime).unwrap_or_default()
}

fn token_key_of(ev: &CanonicalEvent) -> Option<TokenKey> {
    match ev {
        CanonicalEvent::TokenDiscovered(t) if !t.token_address.is_empty() => {
            Some(TokenKey::new(t.chain, &t.token_address))
        }
        CanonicalEvent::Trade(t) if !t.token_address.is_empty() => {
            Some(TokenKey::new(t.chain, &t.token_address))
        }
        CanonicalEvent::Lifecycle(t) if !t.token_address.is_empty() => {
            Some(TokenKey::new(t.chain, &t.token_address))
        }
        _ => None,
    }
}

fn json_text(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    if v.is_number() {
        return Some(v.to_string());
    }
    None
}

fn update_market_from_trade(st: &mut TokenState, trade: &TradeObserved) {
    let meta = &trade.metadata;
    let virt_tok = meta.get("virtual_token_reserves").and_then(json_text);
    let virt_sol = meta.get("virtual_sol_reserves").and_then(json_text);
    let real_tok = meta.get("real_token_reserves").and_then(json_text);
    let real_sol = meta.get("real_sol_reserves").and_then(json_text);
    if virt_tok.is_some() || virt_sol.is_some() {
        if st.baseline_virtual_token.is_none() {
            st.baseline_virtual_token = virt_tok.clone();
        }
        let progress = match (&st.baseline_virtual_token, &virt_tok) {
            (Some(base), Some(cur)) => {
                let sold = super::amt::sub_sat_raw(base, cur);
                ratio_bps(&sold, base)
            }
            _ => None,
        };
        st.curve_progress_bps = progress;
        st.market_state = MarketState::BondingCurve(BondingCurveState {
            virtual_token_reserves: virt_tok,
            virtual_sol_reserves: virt_sol,
            real_token_reserves: real_tok,
            real_sol_reserves: real_sol,
            token_total_supply: None,
            curve_progress_bps: progress,
            last_token_amount_raw: Some(trade.base_amount_raw.clone()),
            last_quote_amount_raw: Some(trade.quote_amount_raw.clone()),
            quality: MarketStateQuality::Complete,
        });
        return;
    }
    if let Some(sqrt) = meta.get("sqrtPriceX96").and_then(json_text) {
        let mut v4 = match &st.market_state {
            MarketState::UniswapV4(s) => s.clone(),
            _ => UniswapV4State {
                pool_id: trade.pool.clone(),
                quote_asset: Some(trade.quote_asset.clone()),
                ..Default::default()
            },
        };
        v4.sqrt_price_x96 = Some(sqrt);
        v4.liquidity_raw = meta.get("liquidity").and_then(json_text);
        v4.tick = meta.get("tick").and_then(json_text);
        v4.amount0 = meta.get("amount0").and_then(json_text);
        v4.amount1 = meta.get("amount1").and_then(json_text);
        v4.pool_id = trade.pool.clone().or(v4.pool_id);
        st.market_state = MarketState::UniswapV4(v4);
        if st.lifecycle_state != TokenLifecycleState::GraduationGap {
            st.lifecycle_state = TokenLifecycleState::AmmActive;
        }
        return;
    }
    if trade.launchpad == Launchpad::PumpSwap
        || (matches!(
            st.lifecycle_state,
            TokenLifecycleState::AmmActive | TokenLifecycleState::Migrating
        ) && trade.launchpad != Launchpad::ClankerV4
            && trade.launchpad != Launchpad::PonsV2)
    {
        if let MarketState::ConstantProduct(cp) = &mut st.market_state {
            cp.last_token_amount_raw = Some(trade.base_amount_raw.clone());
            cp.last_quote_amount_raw = Some(trade.quote_amount_raw.clone());
            cp.pool = trade.pool.clone().or(cp.pool.clone());
            cp.quality = MarketStateQuality::Partial;
        } else if trade.pool.is_some() {
            st.market_state = MarketState::ConstantProduct(ConstantProductState {
                pool: trade.pool.clone(),
                token: Some(st.key.token.clone()),
                quote_asset: Some(trade.quote_asset.clone()),
                last_token_amount_raw: Some(trade.base_amount_raw.clone()),
                last_quote_amount_raw: Some(trade.quote_amount_raw.clone()),
                quality: MarketStateQuality::Partial,
                ..Default::default()
            });
        }
    }
}

fn apply_v4_meta(st: &mut TokenState, meta: &serde_json::Value) {
    let sqrt = meta.get("sqrtPriceX96").and_then(|v| v.as_str());
    if sqrt.is_none() && meta.get("currency0").is_none() {
        return;
    }
    let mut v4 = match &st.market_state {
        MarketState::UniswapV4(s) => s.clone(),
        _ => UniswapV4State {
            pool_id: st.pool.clone(),
            quote_asset: st.quote_asset.clone(),
            ..Default::default()
        },
    };
    if let Some(s) = sqrt {
        v4.sqrt_price_x96 = Some(s.to_string());
    }
    if let Some(t) = meta.get("tick").and_then(|v| v.as_str()) {
        v4.tick = Some(t.to_string());
    }
    if let Some(c0) = meta.get("currency0").and_then(|v| v.as_str()) {
        v4.token0 = Some(c0.to_string());
    }
    if let Some(c1) = meta.get("currency1").and_then(|v| v.as_str()) {
        v4.token1 = Some(c1.to_string());
    }
    st.market_state = MarketState::UniswapV4(v4);
}
