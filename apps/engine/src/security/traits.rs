use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::domain::TokenDiscovered;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FastSecurityVerdict {
    Unknown,
    Reject,
    Watch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FastSecurityResult {
    pub verdict: FastSecurityVerdict,
    pub reasons: Vec<String>,
}

#[async_trait]
pub trait SecurityFast: Send + Sync {
    async fn check(&self, token: &TokenDiscovered) -> Result<FastSecurityResult>;
}

pub struct NoopSecurity;

#[async_trait]
impl SecurityFast for NoopSecurity {
    async fn check(&self, _token: &TokenDiscovered) -> Result<FastSecurityResult> {
        Ok(FastSecurityResult {
            verdict: FastSecurityVerdict::Unknown,
            reasons: vec!["phase1_security_not_implemented".into()],
        })
    }
}
