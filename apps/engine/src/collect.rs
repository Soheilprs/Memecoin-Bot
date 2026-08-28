use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tokio::sync::{mpsc, watch};

use crate::config::EngineConfig;
use crate::decoders::DecoderRegistry;
use crate::domain::{Chain, CollectionSession, QualityStatus, SolanaMode, RPC_DEV_WARNING};
use crate::error::{EngineError, Result};
use crate::ingest::evm::collector::{default_topics, EvmLiveCollector};
use crate::ingest::evm::websocket::EvmWsConfig;
use crate::ingest::solana::health::SolanaSlotTracker;
use crate::ingest::solana::provider::{rpc_provider_name, GrpcProviderConfig};
use crate::ingest::solana::rpc::SolanaRpcCollector;
use crate::ingest::solana::yellowstone::{YellowstoneConfig, YellowstoneIngest};
use crate::ingest::ChainIngest;
use crate::metrics::DiscoveryMetrics;
use crate::pipeline::DiscoveryPipeline;
use crate::state::StateEngine;
use crate::storage::postgres::PostgresStore;
use crate::storage::{EventStore, SessionFinish};
use crate::watch::MarketRegistry;

static RPC_DEV_WARNED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectTarget {
    Solana,
    Base,
    Robinhood,
    Evm,
    All,
}

impl CollectTarget {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "solana" => Some(Self::Solana),
            "base" => Some(Self::Base),
            "robinhood" | "rh" => Some(Self::Robinhood),
            "evm" | "robinhood,base" | "base,robinhood" => Some(Self::Evm),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    pub fn chains(self) -> Vec<Chain> {
        match self {
            Self::Solana => vec![Chain::Solana],
            Self::Base => vec![Chain::Base],
            Self::Robinhood => vec![Chain::Robinhood],
            Self::Evm => vec![Chain::Base, Chain::Robinhood],
            Self::All => vec![Chain::Solana, Chain::Base, Chain::Robinhood],
        }
    }
}

pub fn warn_rpc_dev_once() {
    if RPC_DEV_WARNED
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        tracing::warn!("{RPC_DEV_WARNING}");
        eprintln!("{RPC_DEV_WARNING}");
    }
}

#[derive(Debug, Clone, Default)]
pub struct CollectOpts {
    pub paper: bool,
    pub exp001: bool,
    pub restore_prefix: Option<String>,
    pub duration: Option<std::time::Duration>,
    pub censor_since: Option<chrono::DateTime<chrono::Utc>>,
    pub exp_started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub experiment_id: Option<String>,
}

pub async fn run_collect(config: EngineConfig, target: CollectTarget) -> Result<()> {
    run_collect_opts(config, target, CollectOpts::default()).await
}

pub async fn run_collect_opts(
    config: EngineConfig,
    target: CollectTarget,
    opts: CollectOpts,
) -> Result<()> {
    let db = config
        .database_url
        .as_deref()
        .ok_or_else(|| EngineError::Ingest("DATABASE_URL is required for collect".into()))?;
    let store = PostgresStore::connect(db).await?;
    store.migrate().await?;
    let store = Arc::new(store);
    let markets = Arc::new(MarketRegistry::new());
    let persisted = store.load_watched_markets(Chain::Solana).await?;
    if !persisted.is_empty() {
        tracing::info!(count = persisted.len(), "reloaded watched_markets");
        markets.load_all(persisted);
    }
    let (raw_tx, raw_rx) = mpsc::channel(config.channel_capacity);
    let (discovered_tx, mut discovered_rx) = mpsc::channel(1024);
    let (trade_tx, mut trade_rx) = mpsc::channel(4096);
    let (life_tx, mut life_rx) = mpsc::channel(1024);
    let slots = Arc::new(SolanaSlotTracker::new());
    let (pool_tx, pool_rx) = watch::channel(markets.solana_pools());
    let quality = match (target, config.solana_mode) {
        (CollectTarget::Solana, SolanaMode::RpcDev) | (CollectTarget::All, SolanaMode::RpcDev) => {
            QualityStatus::RpcDevIncomplete
        }
        (CollectTarget::Solana, SolanaMode::Historical) => QualityStatus::HistoricalReplay,
        _ => QualityStatus::LiveComplete,
    };
    let state = Arc::new(Mutex::new(StateEngine::live(quality, None)));
    let pipeline = Arc::new(DiscoveryPipeline {
        store: store.clone(),
        registry: DecoderRegistry::production(),
        markets: markets.clone(),
        discovered_tx,
        trade_tx,
        lifecycle_tx: life_tx,
        metrics: DiscoveryMetrics,
        ingest_id: "collect".into(),
        slots: Some(slots.clone()),
        pool_tx: Some(pool_tx),
        state: Some(state.clone()),
    });

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let pipe = pipeline.clone();
    let pipeline_task = tokio::spawn(async move {
        pipe.run_loop(raw_rx).await;
    });
    {
        let (sec_q, sec_rx) = crate::security::queue::SecurityWorkQueue::bounded(256);
        let sec_engine = std::sync::Arc::new(crate::security::SecurityEngine::default());
        let sec_store = store.clone();
        tokio::spawn(crate::security::queue::run_worker(
            sec_rx, sec_engine, sec_store,
        ));
        tokio::spawn(async move {
            while let Some(tok) = discovered_rx.recv().await {
                let ctx = crate::security::SecurityContext::from_token(tok, quality, false);
                let job = crate::security::queue::SecurityJob {
                    priority: crate::security::queue::SecurityPriority::Discovered,
                    ctx,
                };
                let _ = sec_q.submit(job).await;
            }
        });
    }
    {
        let store_w = store.clone();
        tokio::spawn(async move {
            while let Some(trade) = trade_rx.recv().await {
                if matches!(trade.chain, Chain::Base | Chain::Robinhood) {
                    let buy = trade.side == crate::domain::TradeSide::Buy;
                    let _ = store_w
                        .upsert_evm_wallet(
                            &trade.trader,
                            trade.chain,
                            trade.chain_timestamp.unwrap_or(trade.observed_at),
                            buy,
                            &trade.token_address,
                        )
                        .await;
                }
            }
        });
    }
    tokio::spawn(async move { while life_rx.recv().await.is_some() {} });
    let sched = Arc::new(Mutex::new(crate::live::LiveMilestoneScheduler::default()));
    let runtime = Arc::new(tokio::sync::Mutex::new({
        let mut rt = crate::live::LiveResearchRuntime::new_mode(opts.paper, opts.exp001);
        rt.exp_started_at = opts.exp_started_at;
        rt.experiment_id = opts.experiment_id.clone();
        rt
    }));
    match crate::live::hydrate_watched_tokens(&store, &state).await {
        Ok(n) => tracing::info!(hydrated = n, "live token state hydrated from postgres"),
        Err(e) => tracing::warn!(error = %e, "live hydrate failed"),
    }
    if opts.paper {
        let mut rt = runtime.lock().await;
        let n = crate::live::restore_open_positions_prefixed(
            &store,
            &mut rt,
            opts.restore_prefix.as_deref(),
        )
        .await
        .unwrap_or(0);
        tracing::info!(recovered = n, "paper positions restored from postgres");
    }
    let curve_reader = config
        .http_url_for(Chain::Robinhood)
        .filter(|u| !u.is_empty())
        .and_then(|url| crate::ingest::evm::pons_curve::PonsCurveReader::new(url).ok())
        .map(std::sync::Arc::new);
    if curve_reader.is_some() {
        tracing::info!("pons curve reader enabled (read-only eth_call getters)");
    } else if target.chains().contains(&Chain::Robinhood) {
        tracing::warn!("ROBINHOOD_HTTP_URL missing; paper fills will not obtain curve reserves");
    }
    let tick_task = {
        let st = state.clone();
        let sched = sched.clone();
        let store_t = store.clone();
        let rt = runtime.clone();
        let curve_r = curve_reader.clone();
        let mut sd = shutdown_rx.clone();
        let censor_since = opts.censor_since;
        let exp_id = opts.experiment_id.clone();
        tokio::spawn(async move {
            let mut intv = tokio::time::interval(std::time::Duration::from_millis(250));
            let mut last_hb = std::time::Instant::now();
            loop {
                tokio::select! {
                    _ = sd.changed() => {
                        if *sd.borrow() {
                            break;
                        }
                    }
                    _ = intv.tick() => {
                        let mut run = rt.lock().await;
                        let _ = crate::live::live_tick_once(
                            &st,
                            &sched,
                            store_t.as_ref(),
                            &mut run,
                            Some(store_t.as_ref()),
                            curve_r.as_deref(),
                        )
                        .await;
                        drop(run);
                        if let Some(id) = &exp_id {
                            if last_hb.elapsed() >= std::time::Duration::from_secs(30) {
                                last_hb = std::time::Instant::now();
                                let _ = store_t
                                    .heartbeat_observation(id, chrono::Utc::now(), None, true)
                                    .await;
                            }
                        }
                    }
                }
            }
            if let Some(id) = &exp_id {
                let _ = store_t
                    .close_open_observation_interval(
                        id,
                        chrono::Utc::now(),
                        "VALID",
                        Some("process_stop"),
                    )
                    .await;
            }
            let mut run = rt.lock().await;
            crate::live::end_session(&mut run);
            for p in &run.positions {
                if let Err(e) = store_t.update_paper_position(p).await {
                    tracing::warn!(error = %e, token = %p.token, "failed to persist session-end position");
                }
            }
            if let Ok(n) = store_t.censor_pending_outcomes_since(censor_since).await {
                if n > 0 {
                    crate::metrics::DiscoveryMetrics::prospective_outcome_censored();
                    tracing::info!(
                        censored = n,
                        "pending descriptive outcomes marked CENSORED_SESSION_END"
                    );
                }
            }
        })
    };

    let mut handles = Vec::new();
    let mut session_ids: Vec<(i64, Option<SolanaMode>, &'static str)> = Vec::new();
    for chain in target.chains() {
        match chain {
            Chain::Solana => match config.solana_mode {
                SolanaMode::Historical => {
                    return Err(EngineError::Ingest(
                        "SOLANA_MODE=historical does not collect live data; use `memecoin-engine replay solana <fixture-dir>`".into(),
                    ));
                }
                SolanaMode::RpcDev => {
                    warn_rpc_dev_once();
                    let rpc_http = config.solana_rpc_url.clone().ok_or_else(|| {
                        EngineError::Ingest("SOLANA_RPC_URL is required for rpc_dev mode".into())
                    })?;
                    let rpc_ws = config.solana_ws_url.clone().unwrap_or_default();
                    let mut session = CollectionSession::start(
                        Chain::Solana,
                        SolanaMode::RpcDev,
                        rpc_provider_name(&rpc_http),
                        Some("free JSON-RPC logsSubscribe; incomplete by design".into()),
                    );
                    session.complete = false;
                    session.quality_status = QualityStatus::RpcDevIncomplete;
                    let sid = store.insert_session(&session).await?;
                    session_ids.push((sid, Some(SolanaMode::RpcDev), "solana:rpc_dev"));
                    DiscoveryMetrics::collection_session(Chain::Solana, "rpc_dev", true);
                    let ingest = SolanaRpcCollector {
                        config: YellowstoneConfig {
                            endpoint: String::new(),
                            x_token: None,
                            ingest_id: "solana:rpc_dev".into(),
                            rpc_http: Some(rpc_http.clone()),
                            rpc_ws: Some(rpc_ws.clone()),
                            explicitly_enabled: false,
                        },
                        rpc_http,
                        rpc_ws,
                        store: store.clone(),
                        metrics: DiscoveryMetrics,
                    };
                    let tx = raw_tx.clone();
                    let sd = shutdown_rx.clone();
                    handles.push(tokio::spawn(async move {
                        if let Err(err) = ingest.run(tx, sd).await {
                            tracing::error!(error = %err, "solana rpc_dev collector exited");
                        }
                    }));
                }
                SolanaMode::Yellowstone => {
                    let grpc = GrpcProviderConfig::from_engine(&config);
                    if grpc.url.is_empty() {
                        return Err(EngineError::Ingest(
                            "SOLANA_MODE=yellowstone requires SOLANA_GRPC_URL".into(),
                        ));
                    }
                    let session = CollectionSession::start(
                        Chain::Solana,
                        SolanaMode::Yellowstone,
                        grpc.provider_name(),
                        Some("research-grade Yellowstone gRPC".into()),
                    );
                    let sid = store.insert_session(&session).await?;
                    session_ids.push((sid, Some(SolanaMode::Yellowstone), "solana:pumpfun"));
                    DiscoveryMetrics::collection_session(Chain::Solana, "yellowstone", false);
                    let ingest = YellowstoneIngest {
                        config: YellowstoneConfig {
                            endpoint: grpc.url,
                            x_token: grpc.token,
                            ingest_id: "solana:pumpfun".into(),
                            rpc_http: config.solana_rpc_url.clone(),
                            rpc_ws: config.solana_ws_url.clone(),
                            explicitly_enabled: true,
                        },
                        store: store.clone(),
                        markets: markets.clone(),
                        metrics: DiscoveryMetrics,
                        shutdown: shutdown_rx.clone(),
                        slots: slots.clone(),
                        pool_rx: pool_rx.clone(),
                    };
                    let tx = raw_tx.clone();
                    handles.push(tokio::spawn(async move {
                        if let Err(err) = ingest.run(tx).await {
                            tracing::error!(error = %err, "solana yellowstone collector exited");
                        }
                    }));
                }
            },
            Chain::Base | Chain::Robinhood => {
                let ws = config.ws_url_for(chain).unwrap_or("").to_string();
                let http = config.http_url_for(chain).unwrap_or_default();
                if ws.is_empty() {
                    tracing::warn!(%chain, "ws url missing; collector disabled");
                    continue;
                }
                let start_block = if !http.is_empty() {
                    let httpc = reqwest::Client::builder()
                        .timeout(std::time::Duration::from_secs(15))
                        .build()
                        .ok();
                    if let Some(c) = httpc {
                        crate::ingest::evm::collector::eth_block_number(&c, &http)
                            .await
                            .ok()
                            .map(|b| b as i64)
                    } else {
                        None
                    }
                } else {
                    None
                };
                let session = CollectionSession {
                    id: None,
                    chain,
                    mode: "live".into(),
                    provider: ws.clone(),
                    started_at: Utc::now(),
                    ended_at: None,
                    start_block,
                    end_block: None,
                    start_slot: None,
                    end_slot: None,
                    complete: true,
                    quality_status: QualityStatus::LiveComplete,
                    gap_count: 0,
                    notes: Some("EVM eth_subscribe + eth_getLogs".into()),
                };
                let sid = store.insert_session(&session).await?;
                let ingest = match chain {
                    Chain::Base => "base:live",
                    Chain::Robinhood => "robinhood:live",
                    Chain::Solana => "solana:pumpfun",
                };
                session_ids.push((sid, None, ingest));
                DiscoveryMetrics::collection_session(chain, "live", false);
                let collector = EvmLiveCollector {
                    config: EvmWsConfig {
                        chain,
                        ws_url: ws,
                        ingest_id: format!("{chain}:live"),
                    },
                    http_url: http,
                    store: store.clone(),
                    markets: markets.clone(),
                    metrics: DiscoveryMetrics,
                    topics: default_topics(chain),
                };
                let tx = raw_tx.clone();
                let sd = shutdown_rx.clone();
                handles.push(tokio::spawn(async move {
                    if let Err(err) = collector.run(tx, sd).await {
                        tracing::error!(error = %err, chain = %chain, "evm collector exited");
                    }
                }));
            }
        }
    }
    drop(raw_tx);

    if let Some(d) = opts.duration {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("shutdown signal received");
            }
            _ = tokio::time::sleep(d) => {
                tracing::info!(secs = d.as_secs(), "collect duration elapsed");
            }
        }
    } else {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("shutdown signal received");
            }
        }
    }
    let _ = shutdown_tx.send(true);
    for h in handles {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), h).await;
    }
    let _ = tokio::time::timeout(std::time::Duration::from_secs(90), tick_task).await;
    drop(pipeline);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), pipeline_task).await;

    for (sid, solana_mode, ingest_id) in session_ids {
        let (complete, quality, gaps, end_slot) = match solana_mode {
            Some(SolanaMode::RpcDev) => {
                let gaps = store
                    .unrecovered_gap_count(Chain::Solana)
                    .await
                    .unwrap_or(0);
                (
                    false,
                    QualityStatus::RpcDevIncomplete,
                    gaps,
                    Some(slots.head() as i64).filter(|v| *v > 0),
                )
            }
            Some(mode) => {
                let gaps = store
                    .unrecovered_gap_count(Chain::Solana)
                    .await
                    .unwrap_or(0);
                let complete = mode.session_complete_by_default() && gaps == 0;
                (
                    complete,
                    if complete {
                        mode.quality_status()
                    } else {
                        QualityStatus::DevelopmentIncomplete
                    },
                    gaps,
                    Some(slots.head() as i64).filter(|v| *v > 0),
                )
            }
            None => (true, QualityStatus::LiveComplete, 0, None),
        };
        let end_block = store
            .load_checkpoint(ingest_id)
            .await
            .ok()
            .flatten()
            .and_then(|c| c.last_block);
        let _ = store
            .finish_session(
                sid,
                SessionFinish {
                    ended_at: Utc::now(),
                    end_block,
                    end_slot,
                    complete,
                    quality_status: quality,
                    gap_count: gaps as i32,
                    notes: None,
                },
            )
            .await;
    }
    Ok(())
}
