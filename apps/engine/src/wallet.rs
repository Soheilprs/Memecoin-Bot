//! Cryptographic EVM address identity only. No smart-money score. No person inference.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::raw_event::normalize_address;
use crate::domain::Chain;
use crate::error::Result;
use crate::storage::postgres::PostgresStore;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalletIdentity {
    pub id: i64,
    pub evm_address: String,
}

pub fn identity_key(address: &str) -> String {
    normalize_address(address)
}

impl PostgresStore {
    pub async fn upsert_evm_wallet(
        &self,
        address: &str,
        chain: Chain,
        at: DateTime<Utc>,
        buy: bool,
        token: &str,
    ) -> Result<i64> {
        if !matches!(chain, Chain::Base | Chain::Robinhood) {
            return Ok(0);
        }
        let addr = identity_key(address);
        if addr.len() < 10 {
            return Ok(0);
        }
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO wallet_identities (evm_address)
            VALUES ($1)
            ON CONFLICT (evm_address) DO UPDATE SET evm_address = EXCLUDED.evm_address
            RETURNING id
            "#,
        )
        .bind(&addr)
        .fetch_one(self.pool())
        .await
        .map_err(|e| crate::error::EngineError::Storage(e.to_string()))?;
        sqlx::query(
            r#"
            INSERT INTO chain_wallet_activity (identity_id, chain, first_seen, last_seen, buy_count, sell_count)
            VALUES ($1,$2,$3,$3,$4,$5)
            ON CONFLICT (identity_id, chain) DO UPDATE SET
                last_seen = GREATEST(chain_wallet_activity.last_seen, EXCLUDED.last_seen),
                buy_count = chain_wallet_activity.buy_count + EXCLUDED.buy_count,
                sell_count = chain_wallet_activity.sell_count + EXCLUDED.sell_count
            "#,
        )
        .bind(id)
        .bind(chain.as_str())
        .bind(at)
        .bind(if buy { 1i64 } else { 0 })
        .bind(if buy { 0i64 } else { 1 })
        .execute(self.pool())
        .await
        .map_err(|e| crate::error::EngineError::Storage(e.to_string()))?;
        sqlx::query(
            r#"
            INSERT INTO wallet_token_activity (
                identity_id, chain, token_address, first_seen, last_seen, buy_count, sell_count
            ) VALUES ($1,$2,$3,$4,$4,$5,$6)
            ON CONFLICT (identity_id, chain, token_address) DO UPDATE SET
                last_seen = GREATEST(wallet_token_activity.last_seen, EXCLUDED.last_seen),
                buy_count = wallet_token_activity.buy_count + EXCLUDED.buy_count,
                sell_count = wallet_token_activity.sell_count + EXCLUDED.sell_count
            "#,
        )
        .bind(id)
        .bind(chain.as_str())
        .bind(token)
        .bind(at)
        .bind(if buy { 1i64 } else { 0 })
        .bind(if buy { 0i64 } else { 1 })
        .execute(self.pool())
        .await
        .map_err(|e| crate::error::EngineError::Storage(e.to_string()))?;
        crate::metrics::DiscoveryMetrics::cross_chain_wallet();
        Ok(id)
    }

    pub async fn count_cross_chain_wallets(&self) -> Result<i64> {
        let n: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM (
                SELECT identity_id FROM chain_wallet_activity
                GROUP BY identity_id
                HAVING COUNT(DISTINCT chain) >= 2
            ) t
            "#,
        )
        .fetch_one(self.pool())
        .await
        .map_err(|e| crate::error::EngineError::Storage(e.to_string()))?;
        Ok(n)
    }
}
