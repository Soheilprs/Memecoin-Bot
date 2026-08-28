//! EXP001 start/status/preflight. Paper only. No keys.

use std::fs;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde_json::json;

use crate::collect::{run_collect_opts, CollectOpts, CollectTarget};
use crate::config::EngineConfig;
use crate::domain::Chain;
use crate::error::Result;
use crate::ingest::evm::pons_curve::PonsCurveReader;
use crate::lab::integrity::check_experiment;
use crate::lab::pons_exp::{
    git_commit, Exp001Lock, Exp001State, ExpRunStatus, EXP001_ID, EXP002_ID,
};
use crate::storage::dbcheck::check_database;
use crate::storage::postgres::PostgresStore;

pub fn lock_path() -> &'static str {
    "research/PONS_PROSPECTIVE_EXP001_LOCK.json"
}
pub fn lock_path_for(id: &str) -> String {
    format!("research/{id}_LOCK.json")
}
pub fn audit_path() -> &'static str {
    "research/PONS_PROSPECTIVE_EXP001_AUDIT.jsonl"
}
pub fn audit_path_for(id: &str) -> String {
    format!("research/{id}_AUDIT.jsonl")
}
pub fn health_path() -> &'static str {
    "research/PONS_PROSPECTIVE_EXP001_HEALTH.json"
}

fn append_audit_file(event: &str, payload: &serde_json::Value) {
    let line = json!({"at": Utc::now(), "event": event, "payload": payload});
    if let Ok(s) = serde_json::to_string(&line) {
        let _ = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(audit_path())
            .and_then(|mut f| {
                use std::io::Write;
                writeln!(f, "{s}")
            });
    }
}

pub async fn cmd_lock(url: &str) -> Result<Exp001State> {
    cmd_lock_id(url, EXP001_ID).await
}

pub async fn cmd_relock_id(url: &str, experiment_id: &str) -> Result<Exp001State> {
    if experiment_id == EXP001_ID {
        return Err(crate::error::EngineError::Ingest(
            "cannot relock INVALIDATED EXP001".into(),
        ));
    }
    let store = PostgresStore::connect(url).await?;
    store.migrate().await?;
    if let Some(existing) = store.load_experiment_state(experiment_id).await? {
        if existing.run_status == ExpRunStatus::Invalidated {
            return Err(crate::error::EngineError::Ingest(format!(
                "cannot relock INVALIDATED {experiment_id}"
            )));
        }
        if existing.started_at.is_some() {
            return Err(crate::error::EngineError::Ingest(
                "cannot relock an experiment that has started_at".into(),
            ));
        }
        let old_hash = existing.config_hash.clone();
        if let Ok(s) = serde_json::to_string_pretty(&existing) {
            let _ = fs::write(format!("research/{experiment_id}_LOCK_SUPERSEDED.json"), s);
        }
        let st = Exp001State::locked_for(experiment_id, git_commit());
        st.verify_lock()
            .map_err(|e| crate::error::EngineError::Ingest(e.into()))?;
        store.upsert_exp001_state(&st).await?;
        store
            .insert_experiment_audit(
                experiment_id,
                "EXPERIMENT_LOCK_SUPERSEDED",
                json!({
                    "old_config_hash": old_hash,
                    "new_config_hash": st.config_hash,
                    "source_tree_hash": st.lock.source_tree_hash,
                }),
            )
            .await?;
        if let Ok(s) = serde_json::to_string_pretty(&st) {
            let _ = fs::write(lock_path_for(experiment_id), s);
        }
        append_audit_file(
            "EXPERIMENT_LOCK_SUPERSEDED",
            &json!({
                "id": experiment_id,
                "old_config_hash": old_hash,
                "new_config_hash": st.config_hash,
            }),
        );
        return Ok(st);
    }
    cmd_lock_id(url, experiment_id).await
}

pub async fn cmd_lock_id(url: &str, experiment_id: &str) -> Result<Exp001State> {
    let store = PostgresStore::connect(url).await?;
    store.migrate().await?;
    if let Some(existing) = store.load_experiment_state(experiment_id).await? {
        existing.verify_lock().map_err(|e| {
            crate::error::EngineError::Ingest(format!("existing lock invalid: {e}"))
        })?;
        return Ok(existing);
    }
    let st = Exp001State::locked_for(experiment_id, git_commit());
    st.verify_lock()
        .map_err(|e| crate::error::EngineError::Ingest(e.into()))?;
    store.upsert_exp001_state(&st).await?;
    store
        .insert_experiment_audit(
            experiment_id,
            "EXPERIMENT_LOCKED",
            json!({
                "config_hash": st.config_hash,
                "source_tree_hash": st.lock.source_tree_hash,
            }),
        )
        .await?;
    if let Ok(s) = serde_json::to_string_pretty(&st) {
        let _ = fs::write(lock_path_for(experiment_id), s);
    }
    append_audit_file(
        "EXPERIMENT_LOCKED",
        &json!({ "config_hash": st.config_hash }),
    );
    Ok(st)
}

pub async fn cmd_status(url: &str) -> Result<crate::lab::pons_exp::Exp001StatusReport> {
    cmd_status_id(url, EXP001_ID).await
}

pub async fn cmd_status_id(
    url: &str,
    experiment_id: &str,
) -> Result<crate::lab::pons_exp::Exp001StatusReport> {
    let store = PostgresStore::connect(url).await?;
    store.migrate().await?;
    let st = store.load_experiment_state(experiment_id).await?;
    let since = st
        .as_ref()
        .and_then(|s| s.started_at)
        .unwrap_or_else(|| Utc::now() - chrono::Duration::days(14));
    let integ = check_experiment(&store, experiment_id).await?;
    let mut report = store.exp001_counts(since).await?;
    report.experiment_id = experiment_id.into();
    report.orders = integ.entry_orders;
    report.fills = integ.entry_fills;
    report.entry_orders = integ.entry_orders;
    report.entry_fills = integ.entry_fills;
    report.exit_orders = integ.exit_orders;
    report.exit_fills = integ.exit_fills;
    report.partial_exit_fills = integ.partial_exit_fills;
    report.failed_exit_attempts = integ.failed_exit_attempts;
    report.positions_opened = integ.positions_opened;
    report.positions_currently_open = integ.positions_open;
    report.session_ended_open = integ.positions_session_ended;
    report.positions_open = integ.positions_open + integ.positions_session_ended;
    report.positions_closed = integ.positions_closed;
    report.valid_uptime_secs = store.valid_uptime_secs(experiment_id).await.unwrap_or(0);
    if st.as_ref().and_then(|s| s.started_at).is_none() {
        report.tokens = 0;
        report.signals = 0;
        report.outcomes_pending = 0;
        report.outcomes_mature = 0;
        report.outcomes_censored = 0;
    }
    if let Some(s) = st {
        report.status = s.run_status.as_str().into();
        report.config_hash = Some(s.config_hash);
        report.git_commit = s.git_commit.or(s.lock.source_tree_hash.clone());
        report.started_at = s.started_at;
        report.start_block = s.start_block;
        report.restarts = s.restarts;
        if let Some(t0) = s.started_at {
            report.elapsed_secs = (Utc::now() - t0).num_seconds();
        }
        if report.elapsed_secs > 0 {
            report.note = format!(
                "operational only; coverage={:.4} threshold=0.95; entry_fills={} positions_opened={}",
                report.valid_uptime_secs as f64 / report.elapsed_secs as f64,
                integ.entry_fills,
                integ.positions_opened
            );
        }
    } else {
        report.status = "NOT_LOCKED".into();
    }
    if let Ok(s) = serde_json::to_string_pretty(&report) {
        let _ = fs::write(format!("research/{experiment_id}_HEALTH.json"), s);
    }
    Ok(report)
}

pub async fn cmd_invalidate_exp001(url: &str) -> Result<Exp001State> {
    cmd_invalidate_id(
        url,
        EXP001_ID,
        "PREFLIGHT_DATA_INTEGRITY: PRE_START_HYDRATED_TOKEN_RISK, VALID_UPTIME_NOT_ACCOUNTED, POSITION_FILL_COUNT_REQUIRES_RECONCILIATION",
    )
    .await
}

pub async fn cmd_invalidate_id(
    url: &str,
    experiment_id: &str,
    reason: &str,
) -> Result<Exp001State> {
    let store = PostgresStore::connect(url).await?;
    store.migrate().await?;
    let mut st = store
        .load_experiment_state(experiment_id)
        .await?
        .ok_or_else(|| crate::error::EngineError::Ingest(format!("{experiment_id} not found")))?;
    let n_pos = store
        .session_end_experiment_positions(experiment_id)
        .await
        .unwrap_or(0);
    let n_censor = if let Some(t) = st.started_at {
        store
            .censor_pending_outcomes_since(Some(t))
            .await
            .unwrap_or(0)
    } else {
        0
    };
    let _ = store
        .close_open_observation_interval(experiment_id, Utc::now(), "INVALID", Some("INVALIDATED"))
        .await;
    st.run_status = ExpRunStatus::Invalidated;
    st.pause_reason = Some(reason.into());
    store.upsert_exp001_state(&st).await?;
    store
        .insert_experiment_audit(
            experiment_id,
            "EXPERIMENT_INVALIDATED",
            json!({
                "reason": reason,
                "session_ended_open": n_pos,
                "censored": n_censor,
                "rows_deleted": 0,
                "started_at": st.started_at,
                "start_block": st.start_block,
            }),
        )
        .await?;
    append_audit_file(
        "EXPERIMENT_INVALIDATED",
        &json!({ "id": experiment_id, "reason": reason, "rows_deleted": 0 }),
    );
    if let Ok(s) = serde_json::to_string_pretty(&st) {
        let _ = fs::write(lock_path_for(experiment_id), &s);
        if experiment_id == EXP001_ID {
            let _ = fs::write(lock_path(), &s);
        }
    }
    Ok(st)
}

pub async fn cmd_start(config: EngineConfig) -> Result<()> {
    cmd_start_id(config, EXP002_ID).await
}

pub async fn cmd_start_id(config: EngineConfig, experiment_id: &str) -> Result<()> {
    cmd_start_id_for(config, experiment_id, None).await
}

pub async fn cmd_start_id_for(
    config: EngineConfig,
    experiment_id: &str,
    duration: Option<Duration>,
) -> Result<()> {
    if experiment_id == EXP001_ID {
        return Err(crate::error::EngineError::Ingest(
            "PONS_PROSPECTIVE_EXP001 is INVALIDATED and cannot restart as the final test".into(),
        ));
    }
    if experiment_id == EXP002_ID {
        return Err(crate::error::EngineError::Ingest(
            "PONS_PROSPECTIVE_EXP002 is INVALIDATED (RESTART_ENTRY_IDEMPOTENCY) and cannot restart"
                .into(),
        ));
    }
    let url = config
        .database_url
        .clone()
        .ok_or_else(|| crate::error::EngineError::Ingest("DATABASE_URL required".into()))?;
    let db = check_database(&url).await;
    if db.blocked {
        return Err(crate::error::EngineError::Ingest(db.message));
    }
    let store = PostgresStore::connect(&url).await?;
    store.migrate().await?;
    let mut st = match store.load_experiment_state(experiment_id).await? {
        Some(s) => s,
        None => cmd_lock_id(&url, experiment_id).await?,
    };
    if st.run_status == ExpRunStatus::Invalidated {
        return Err(crate::error::EngineError::Ingest(format!(
            "{experiment_id} is INVALIDATED ({}) and cannot restart",
            st.pause_reason.as_deref().unwrap_or("unknown")
        )));
    }
    st.verify_lock()
        .map_err(|e| crate::error::EngineError::Ingest(e.into()))?;
    if st.started_at.is_none() {
        st.started_at = Some(Utc::now());
        st.start_wall_time = st.started_at;
        st.run_status = ExpRunStatus::Running;
        if let Some(http) = config.http_url_for(Chain::Robinhood) {
            if let Ok(r) = PonsCurveReader::new(http) {
                if let Ok(h) = r.head_block().await {
                    st.start_block = Some(h);
                }
            }
        }
        store
            .insert_experiment_audit(
                experiment_id,
                "EXPERIMENT_STARTED",
                json!({ "start_block": st.start_block }),
            )
            .await?;
        append_audit_file(
            "EXPERIMENT_STARTED",
            &json!({ "start_block": st.start_block, "id": experiment_id }),
        );
        let _ = store
            .open_observation_interval(
                experiment_id,
                st.started_at.unwrap_or_else(Utc::now),
                "VALID",
            )
            .await;
    } else {
        st.restarts += 1;
        st.run_status = ExpRunStatus::Running;
        store
            .insert_experiment_audit(
                experiment_id,
                "PROCESS_RESTARTED",
                json!({ "restarts": st.restarts }),
            )
            .await?;
        append_audit_file("PROCESS_RESTARTED", &json!({ "restarts": st.restarts }));
        let _ = store
            .close_open_observation_interval(experiment_id, Utc::now(), "PARTIAL", Some("restart"))
            .await;
        let _ = store
            .open_observation_interval(experiment_id, Utc::now(), "VALID")
            .await;
    }
    st.last_heartbeat = Some(Utc::now());
    store.upsert_exp001_state(&st).await?;
    if let Ok(s) = serde_json::to_string_pretty(&st) {
        let _ = fs::write(lock_path_for(experiment_id), s);
    }
    let started = st.started_at;
    let opts = CollectOpts {
        paper: true,
        exp001: true,
        restore_prefix: Some(experiment_id.into()),
        duration,
        censor_since: started,
        exp_started_at: started,
        experiment_id: Some(experiment_id.into()),
    };
    tracing::info!(
        experiment = experiment_id,
        config_hash = %st.config_hash,
        "prospective paper start: 15 arms, no broadcast, no backfill"
    );
    let result = run_collect_opts(config, CollectTarget::Evm, opts).await;
    if duration.is_some() {
        let _ = store.session_end_experiment_positions(experiment_id).await;
        let _ = store
            .close_open_observation_interval(
                experiment_id,
                Utc::now(),
                "VALID",
                Some("bounded_run"),
            )
            .await;
        if let Some(t) = started {
            let _ = store.censor_pending_outcomes_since(Some(t)).await;
        }
        if let Ok(Some(mut ended)) = store.load_experiment_state(experiment_id).await {
            ended.run_status = ExpRunStatus::PausedOperational;
            ended.pause_reason = Some("bounded_run_complete".into());
            let _ = store.upsert_exp001_state(&ended).await;
            if let Ok(s) = serde_json::to_string_pretty(&ended) {
                let _ = fs::write(lock_path_for(experiment_id), s);
            }
        }
    }
    result
}

pub async fn cmd_integrity(
    url: &str,
    experiment_id: &str,
) -> Result<crate::lab::integrity::IntegrityReport> {
    let store = PostgresStore::connect(url).await?;
    store.migrate().await?;
    check_experiment(&store, experiment_id).await
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PreflightReport {
    pub verdict: String,
    pub db_ok: bool,
    pub curve_ok: bool,
    pub historical_eth_call: bool,
    pub rpc: serde_json::Value,
    pub censor: serde_json::Value,
    pub template_hash_status: String,
    pub blockers: Vec<String>,
}

pub async fn cmd_preflight(config: EngineConfig) -> Result<PreflightReport> {
    let url = config
        .database_url
        .clone()
        .ok_or_else(|| crate::error::EngineError::Ingest("DATABASE_URL required".into()))?;
    let db = check_database(&url).await;
    let mut blockers = Vec::new();
    if db.blocked {
        blockers.push(db.message.clone());
    }
    let store = PostgresStore::connect(&url).await?;
    store.migrate().await?;

    let mut curve_ok = false;
    let mut hist = false;
    let mut rpc_stats = json!({"eth_call": 0});
    if let Some(http) = config.http_url_for(Chain::Robinhood) {
        if let Ok(reader) = PonsCurveReader::new(&http) {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT token_address, curve FROM token_discovered WHERE chain='robinhood' AND curve IS NOT NULL ORDER BY observed_at DESC LIMIT 5",
            )
            .fetch_all(store.pool())
            .await
            .unwrap_or_default();
            let mut ok = 0u32;
            let mut fail = 0u32;
            let mut lats: Vec<u128> = Vec::new();
            for (tok, curve) in &rows {
                let t0 = Instant::now();
                match reader.read(tok, curve, None).await {
                    Ok(st) => {
                        ok += 1;
                        lats.push(t0.elapsed().as_millis());
                        if st.state_quality.research_valid_live_paper() {
                            curve_ok = true;
                        }
                    }
                    Err(_) => fail += 1,
                }
            }
            if let Some((_, curve)) = rows.first() {
                if let Ok(cap) = reader.probe_historical(curve).await {
                    hist = cap.supported;
                }
            }
            lats.sort();
            let p50 = lats.get(lats.len() / 2).copied().unwrap_or(0);
            let p95 = lats
                .get((lats.len().saturating_mul(95) / 100).min(lats.len().saturating_sub(1)))
                .copied()
                .unwrap_or(0);
            rpc_stats = json!({
                "eth_call_batches": ok + fail,
                "success": ok,
                "failure": fail,
                "success_rate": if ok+fail==0 { 0.0 } else { ok as f64 / (ok+fail) as f64 },
                "timeout_rate": 0.0,
                "rate_limit_rate": 0.0,
                "p50_ms": p50,
                "p95_ms": p95,
                "historical_eth_call": hist,
            });
        } else {
            blockers.push("curve reader init failed".into());
        }
    } else {
        blockers.push("ROBINHOOD_HTTP_URL missing".into());
    }

    let since = Utc::now();
    let censor_opts = CollectOpts {
        paper: true,
        exp001: false,
        restore_prefix: Some("PREFLIGHT_NO_RESTORE".into()),
        duration: Some(Duration::from_secs(90)),
        censor_since: Some(since),
        exp_started_at: None,
        experiment_id: None,
    };
    let mut censor = json!({"ran": false});
    if !db.blocked {
        let _ = run_collect_opts(config.clone(), CollectTarget::Robinhood, censor_opts).await;
        let _ = store.censor_pending_outcomes_since(Some(since)).await;
        let _ = sqlx::query(
            "UPDATE simulated_positions SET status='SESSION_ENDED_OPEN' WHERE status='OPEN' AND strategy_policy_id='PIPELINE_SMOKE_POLICY' AND opened_at >= $1",
        )
        .bind(since)
        .execute(store.pool())
        .await;
        let ended: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM simulated_positions WHERE status='SESSION_ENDED_OPEN' AND strategy_policy_id='PIPELINE_SMOKE_POLICY'",
        )
        .fetch_one(store.pool())
        .await
        .unwrap_or(0);
        let censored: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM descriptive_token_outcomes WHERE maturity='CENSORED_SESSION_END' AND created_at >= $1",
        )
        .bind(since)
        .fetch_one(store.pool())
        .await
        .unwrap_or(0);
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM descriptive_token_outcomes WHERE maturity='PENDING' AND created_at >= $1",
        )
        .bind(since)
        .fetch_one(store.pool())
        .await
        .unwrap_or(0);
        censor = json!({
            "ran": true,
            "session_ended_open": ended,
            "censored_session_end": censored,
            "pending_after": pending,
        });
        if ended == 0 && censored == 0 {
            blockers
                .push("censor/session-end not observed (no new pending/open rows in 90s)".into());
        }
    }

    if !curve_ok {
        blockers.push("no EXACT_BLOCK_READ curve sample".into());
    }
    let verdict = if blockers.is_empty() {
        "PREFLIGHT_PASS"
    } else {
        "PREFLIGHT_FAIL"
    };
    Ok(PreflightReport {
        verdict: verdict.into(),
        db_ok: !db.blocked,
        curve_ok,
        historical_eth_call: hist,
        rpc: rpc_stats,
        censor,
        template_hash_status: Exp001Lock::predeclared().pons_token_runtime_hash_status,
        blockers,
    })
}
