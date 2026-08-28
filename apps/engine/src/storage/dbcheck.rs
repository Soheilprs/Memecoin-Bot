//! Fail closed before live collectors. Never print credentials.

use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;

use super::postgres::PostgresStore;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DbCheckReport {
    pub ok: bool,
    pub blocked: bool,
    pub connected: bool,
    pub migrated: bool,
    pub smoke_write_read: bool,
    pub message: String,
}

impl DbCheckReport {
    pub fn blocked(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            blocked: true,
            connected: false,
            migrated: false,
            smoke_write_read: false,
            message: sanitize_db_error(&msg.into()),
        }
    }
}

pub fn sanitize_db_error(raw: &str) -> String {
    let mut s = raw.to_string();
    if let Ok(url) = std::env::var("DATABASE_URL") {
        if let Some(at) = url.find('@') {
            if let Some(scheme) = url.find("://") {
                let creds = &url[scheme + 3..at];
                if !creds.is_empty() {
                    s = s.replace(creds, "***");
                }
            }
        }
        s = s.replace(&url, "DATABASE_URL");
    }
    s = s.replace("memecoin:memecoin", "***:***");
    if s.contains("password authentication failed") {
        return "BLOCKED_DATABASE: password authentication failed (sanitized)".into();
    }
    format!("BLOCKED_DATABASE: {s}")
}

pub async fn check_database(database_url: &str) -> DbCheckReport {
    let pool = match PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(database_url)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            return DbCheckReport::blocked(e.to_string());
        }
    };
    let store = PostgresStore::from_pool(pool.clone());
    if let Err(e) = store.migrate().await {
        return DbCheckReport {
            ok: false,
            blocked: true,
            connected: true,
            migrated: false,
            smoke_write_read: false,
            message: sanitize_db_error(&e.to_string()),
        };
    }
    let marker = format!("phase72-smoke-{}", chrono::Utc::now().timestamp_millis());
    let ins = sqlx::query("INSERT INTO wallet_identities (evm_address) VALUES ($1) ON CONFLICT (evm_address) DO NOTHING")
        .bind(&marker)
        .execute(&pool)
        .await;
    if let Err(e) = ins {
        return DbCheckReport {
            ok: false,
            blocked: true,
            connected: true,
            migrated: true,
            smoke_write_read: false,
            message: sanitize_db_error(&e.to_string()),
        };
    }
    let found: Option<String> =
        sqlx::query_scalar("SELECT evm_address FROM wallet_identities WHERE evm_address = $1")
            .bind(&marker)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
    let _ = sqlx::query("DELETE FROM wallet_identities WHERE evm_address = $1")
        .bind(&marker)
        .execute(&pool)
        .await;
    if found.as_deref() != Some(marker.as_str()) {
        return DbCheckReport {
            ok: false,
            blocked: true,
            connected: true,
            migrated: true,
            smoke_write_read: false,
            message: "BLOCKED_DATABASE: smoke write/read mismatch".into(),
        };
    }
    DbCheckReport {
        ok: true,
        blocked: false,
        connected: true,
        migrated: true,
        smoke_write_read: true,
        message: "ok".into(),
    }
}
