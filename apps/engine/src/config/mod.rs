use crate::domain::{Chain, SolanaMode};

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub database_url: Option<String>,
    pub solana_mode: SolanaMode,
    pub solana_yellowstone_endpoint: Option<String>,
    pub solana_x_token: Option<String>,
    pub solana_rpc_url: Option<String>,
    pub solana_ws_url: Option<String>,
    pub base_ws_url: Option<String>,
    pub base_http_url: Option<String>,
    pub robinhood_ws_url: Option<String>,
    pub robinhood_http_url: Option<String>,
    pub metrics_addr: Option<String>,
    pub channel_capacity: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            database_url: None,
            solana_mode: SolanaMode::RpcDev,
            solana_yellowstone_endpoint: None,
            solana_x_token: None,
            solana_rpc_url: None,
            solana_ws_url: None,
            base_ws_url: None,
            base_http_url: None,
            robinhood_ws_url: None,
            robinhood_http_url: None,
            metrics_addr: None,
            channel_capacity: 4096,
        }
    }
}

impl EngineConfig {
    pub fn from_env() -> Self {
        Self::from_env_with_mode(None)
    }

    pub fn from_env_with_mode(cli_mode: Option<&str>) -> Self {
        let solana_grpc = std::env::var("SOLANA_GRPC_URL")
            .ok()
            .or_else(|| std::env::var("SOLANA_YELLOWSTONE_ENDPOINT").ok());
        let solana_token = std::env::var("SOLANA_GRPC_TOKEN")
            .ok()
            .or_else(|| std::env::var("SOLANA_YELLOWSTONE_X_TOKEN").ok());
        let solana_mode =
            SolanaMode::resolve(cli_mode, std::env::var("SOLANA_MODE").ok().as_deref())
                .unwrap_or(SolanaMode::RpcDev);
        Self {
            database_url: std::env::var("DATABASE_URL").ok(),
            solana_mode,
            solana_yellowstone_endpoint: solana_grpc,
            solana_x_token: solana_token,
            solana_rpc_url: std::env::var("SOLANA_RPC_URL").ok(),
            solana_ws_url: std::env::var("SOLANA_WS_URL").ok(),
            base_ws_url: std::env::var("BASE_WS_URL").ok(),
            base_http_url: std::env::var("BASE_HTTP_URL").ok(),
            robinhood_ws_url: std::env::var("ROBINHOOD_WS_URL").ok(),
            robinhood_http_url: std::env::var("ROBINHOOD_HTTP_URL").ok(),
            metrics_addr: std::env::var("METRICS_ADDR").ok(),
            channel_capacity: std::env::var("INGEST_CHANNEL_CAPACITY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(4096),
        }
    }

    pub fn ws_url_for(&self, chain: Chain) -> Option<&str> {
        match chain {
            Chain::Base => self.base_ws_url.as_deref(),
            Chain::Robinhood => self.robinhood_ws_url.as_deref(),
            Chain::Solana => self.solana_ws_url.as_deref(),
        }
    }

    pub fn http_url_for(&self, chain: Chain) -> Option<String> {
        match chain {
            Chain::Base => self.base_http_url.clone().or_else(|| {
                self.base_ws_url
                    .as_deref()
                    .map(crate::ingest::evm::collector::http_from_ws)
            }),
            Chain::Robinhood => self.robinhood_http_url.clone().or_else(|| {
                self.robinhood_ws_url
                    .as_deref()
                    .map(crate::ingest::evm::collector::http_from_ws)
            }),
            Chain::Solana => self.solana_rpc_url.clone(),
        }
    }
}
