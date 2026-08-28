use crate::domain::{
    CanonicalEvent, Chain, EventOrderKey, LifecycleObserved, TokenDiscovered, TradeObserved,
};

/// Deterministic state order. Never timestamp-only.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StateOrder {
    pub chain: Chain,
    pub block_or_slot: u64,
    pub transaction_index: u64,
    pub log_or_ix: u64,
    pub inner: u64,
    pub event_index: u64,
    pub event_id: String,
}

impl StateOrder {
    pub fn from_trade(t: &TradeObserved, event_index: u64) -> Self {
        Self {
            chain: t.chain,
            block_or_slot: t.block_number.or(t.slot).unwrap_or(0),
            transaction_index: t.transaction_index.unwrap_or(0),
            log_or_ix: t
                .log_index
                .unwrap_or(t.instruction_index.map(|v| v as u64).unwrap_or(0)),
            inner: t.inner_instruction_index.map(|v| v as u64 + 1).unwrap_or(0),
            event_index,
            event_id: t.event_id.clone(),
        }
    }

    pub fn from_life(l: &LifecycleObserved, event_index: u64) -> Self {
        Self {
            chain: l.chain,
            block_or_slot: l.block_number.or(l.slot).unwrap_or(0),
            transaction_index: l.transaction_index.unwrap_or(0),
            log_or_ix: l
                .log_index
                .unwrap_or(l.instruction_index.map(|v| v as u64).unwrap_or(0)),
            inner: l.inner_instruction_index.map(|v| v as u64 + 1).unwrap_or(0),
            event_index,
            event_id: l.event_id.clone(),
        }
    }

    pub fn from_token(t: &TokenDiscovered, event_index: u64) -> Self {
        Self {
            chain: t.chain,
            block_or_slot: t.block_number.or(t.slot).unwrap_or(0),
            transaction_index: 0,
            log_or_ix: t
                .log_index
                .unwrap_or(t.instruction_index.map(|v| v as u64).unwrap_or(0)),
            inner: t.inner_instruction_index.map(|v| v as u64 + 1).unwrap_or(0),
            event_index,
            event_id: t.raw_event_id.clone(),
        }
    }

    pub fn from_canonical(ev: &CanonicalEvent, event_index: u64) -> Self {
        match ev {
            CanonicalEvent::TokenDiscovered(t) => Self::from_token(t, event_index),
            CanonicalEvent::Trade(t) => Self::from_trade(t, event_index),
            CanonicalEvent::Lifecycle(l) => Self::from_life(l, event_index),
        }
    }

    pub fn encoded(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}:{}",
            self.chain.as_str(),
            self.block_or_slot,
            self.transaction_index,
            self.log_or_ix,
            self.inner,
            self.event_index,
            self.event_id
        )
    }

    pub fn to_event_order_key(&self) -> EventOrderKey {
        EventOrderKey {
            chain: self.chain,
            block_or_slot: self.block_or_slot,
            transaction_index: self.transaction_index,
            log_or_ix: self.log_or_ix,
            inner: self.inner,
            event_id: self.event_id.clone(),
        }
    }
}
