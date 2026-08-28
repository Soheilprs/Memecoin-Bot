use serde_json::{json, Value};

use crate::error::{EngineError, Result};

pub async fn http_jsonrpc(
    http: &reqwest::Client,
    url: &str,
    method: &str,
    params: Value,
) -> Result<Value> {
    let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
    let resp = http
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| EngineError::Rpc(e.to_string()))?;
    let v: Value = resp
        .json()
        .await
        .map_err(|e| EngineError::Rpc(e.to_string()))?;
    if let Some(err) = v.get("error") {
        return Err(EngineError::Rpc(err.to_string()));
    }
    v.get("result")
        .cloned()
        .ok_or_else(|| EngineError::Rpc("missing result".into()))
}

pub fn hex_u64(v: &Value) -> Option<u64> {
    match v {
        Value::String(s) => u64::from_str_radix(s.trim_start_matches("0x"), 16).ok(),
        Value::Number(n) => n.as_u64(),
        _ => None,
    }
}
