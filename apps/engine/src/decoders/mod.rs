use crate::domain::{CanonicalEvent, RawEvent, TokenDiscovered};
use crate::error::Result;

pub mod clanker_v4;
pub mod evm_util;
pub mod pons_v2;
pub mod pump_corpus;
pub mod pumpfun;
pub mod pumpswap;
pub mod solana_buf;
pub mod uniswap_v4;

pub use clanker_v4::ClankerV4Decoder;
pub use pons_v2::PonsV2Decoder;
pub use pump_corpus::PumpCorpusDecoder;
pub use pumpfun::PumpfunDecoder;
pub use pumpswap::PumpSwapDecoder;
pub use uniswap_v4::UniswapV4Decoder;

pub trait Decoder: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn matches(&self, raw: &RawEvent) -> bool;
    fn decode(&self, raw: &RawEvent) -> Result<Vec<CanonicalEvent>>;
}

#[derive(Debug)]
pub enum DecodeOutcome {
    Events(Vec<CanonicalEvent>),
    Unknown,
}

impl DecodeOutcome {
    pub fn tokens(&self) -> Vec<&TokenDiscovered> {
        match self {
            DecodeOutcome::Events(events) => events.iter().filter_map(|e| e.as_token()).collect(),
            DecodeOutcome::Unknown => Vec::new(),
        }
    }

    pub fn into_token(self) -> Option<TokenDiscovered> {
        match self {
            DecodeOutcome::Events(events) => events.into_iter().find_map(|e| e.into_token()),
            DecodeOutcome::Unknown => None,
        }
    }
}

pub struct DecoderRegistry {
    decoders: Vec<Box<dyn Decoder>>,
}

impl DecoderRegistry {
    pub fn new(decoders: Vec<Box<dyn Decoder>>) -> Self {
        Self { decoders }
    }

    pub fn production() -> Self {
        Self::new(vec![
            Box::new(pump_corpus::PumpCorpusDecoder::pinned()),
            Box::new(pumpfun::PumpfunDecoder::pinned()),
            Box::new(pumpswap::PumpSwapDecoder::pinned()),
            Box::new(pons_v2::PonsV2Decoder::pinned()),
            Box::new(clanker_v4::ClankerV4Decoder::pinned()),
            Box::new(uniswap_v4::UniswapV4Decoder::pinned()),
        ])
    }

    pub fn decode(&self, raw: &RawEvent) -> Result<DecodeOutcome> {
        let mut matched: Option<&dyn Decoder> = None;
        for decoder in &self.decoders {
            if decoder.matches(raw) {
                matched = Some(decoder.as_ref());
                break;
            }
        }
        match matched {
            None => Ok(DecodeOutcome::Unknown),
            Some(decoder) => {
                if let Some(requested) = raw.decoder_version.as_deref() {
                    if requested != decoder.version() {
                        return Err(crate::error::EngineError::DecoderVersionMismatch {
                            protocol: decoder.name().to_string(),
                            requested: requested.to_string(),
                            pinned: decoder.version().to_string(),
                        });
                    }
                }
                let events = decoder.decode(raw)?;
                if events.is_empty() {
                    Ok(DecodeOutcome::Unknown)
                } else {
                    Ok(DecodeOutcome::Events(events))
                }
            }
        }
    }
}
