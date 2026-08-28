//! Yellowstone transport settings. Domain decoding does not branch on vendor.

use crate::config::EngineConfig;

/// URL, auth metadata, and host quirks only. Pump decoding stays provider-neutral.
#[derive(Debug, Clone)]
pub struct GrpcProviderConfig {
    pub url: String,
    pub token: Option<String>,
}

impl GrpcProviderConfig {
    pub fn from_engine(config: &EngineConfig) -> Self {
        Self {
            url: config
                .solana_yellowstone_endpoint
                .clone()
                .unwrap_or_default(),
            token: config.solana_x_token.clone(),
        }
    }

    pub fn provider_name(&self) -> String {
        url::Url::parse(&self.url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| "yellowstone".into())
    }
}

pub fn rpc_provider_name(rpc_http: &str) -> String {
    url::Url::parse(rpc_http)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "json-rpc".into())
}
