use super::lifecycle::LifecycleObserved;
use super::token_discovered::TokenDiscovered;
use super::trade::TradeObserved;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalEvent {
    TokenDiscovered(Box<TokenDiscovered>),
    Trade(Box<TradeObserved>),
    Lifecycle(Box<LifecycleObserved>),
}

impl CanonicalEvent {
    pub fn as_token(&self) -> Option<&TokenDiscovered> {
        match self {
            CanonicalEvent::TokenDiscovered(t) => Some(t),
            _ => None,
        }
    }

    pub fn into_token(self) -> Option<TokenDiscovered> {
        match self {
            CanonicalEvent::TokenDiscovered(t) => Some(*t),
            _ => None,
        }
    }

    pub fn as_trade(&self) -> Option<&TradeObserved> {
        match self {
            CanonicalEvent::Trade(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_lifecycle(&self) -> Option<&LifecycleObserved> {
        match self {
            CanonicalEvent::Lifecycle(t) => Some(t),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DecodedBatch {
    pub tokens: Vec<TokenDiscovered>,
    pub trades: Vec<TradeObserved>,
    pub lifecycle: Vec<LifecycleObserved>,
}

impl DecodedBatch {
    pub fn from_events(events: Vec<CanonicalEvent>) -> Self {
        let mut batch = Self::default();
        for event in events {
            match event {
                CanonicalEvent::TokenDiscovered(t) => batch.tokens.push(*t),
                CanonicalEvent::Trade(t) => batch.trades.push(*t),
                CanonicalEvent::Lifecycle(t) => batch.lifecycle.push(*t),
            }
        }
        batch
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty() && self.trades.is_empty() && self.lifecycle.is_empty()
    }
}
