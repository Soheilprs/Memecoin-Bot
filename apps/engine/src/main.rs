use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use memecoin_engine::collect::{run_collect, CollectTarget};
use memecoin_engine::config::EngineConfig;
use memecoin_engine::registry::verified_factories;
use memecoin_engine::replay::format_report;
use memecoin_engine::storage::memory::MemoryStore;
use memecoin_engine::storage::postgres::PostgresStore;
use memecoin_engine::storage::EventStore;
use memecoin_engine::watch::MarketRegistry;
use tracing_subscriber::EnvFilter;

fn load_dotenv() {
    let path = std::path::Path::new(".env");
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if std::env::var(k).is_err() {
            std::env::set_var(k, v);
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "memecoin-engine",
    about = "Phase 2.1B collectors and historical replay"
)]
struct Args {
    #[arg(long)]
    print_registry: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run collectors (read-only). Solana requires --mode or SOLANA_MODE.
    Collect {
        #[arg(value_name = "CHAIN", default_value = "all")]
        chain: String,
        /// Solana ingest mode: historical | rpc-dev | yellowstone
        #[arg(long)]
        mode: Option<String>,
    },
    /// Offline replay through the production DecoderRegistry.
    Replay {
        #[arg(value_name = "CHAIN")]
        chain: String,
        #[arg(value_name = "FIXTURE_DIR")]
        fixture_dir: PathBuf,
        /// Persist canonical rows into DATABASE_URL when set.
        #[arg(long)]
        persist: bool,
        /// Run the Phase 3 state engine and emit snapshots.
        #[arg(long)]
        snapshots: bool,
        /// Compute Phase 5 feature vectors and candidate transitions (implies --snapshots).
        #[arg(long)]
        features: bool,
    },
    /// Decoded Pump.fun research corpus (JSONL). Does not mark HISTORICAL_REPLAY unless valid.
    Corpus {
        #[command(subcommand)]
        cmd: CorpusCmd,
    },
    /// Read-only security assessment (no broadcast, no keys).
    Security {
        #[command(subcommand)]
        cmd: SecurityCmd,
    },
    /// Research export (features/candidates). No trading.
    Research {
        #[command(subcommand)]
        cmd: ResearchCmd,
    },
    /// Simulated execution only. Never broadcasts. No keys.
    Simulate {
        #[command(subcommand)]
        cmd: SimulateCmd,
    },
}

#[derive(Subcommand, Debug)]
enum SimulateCmd {
    Historical {
        fixture_dir: PathBuf,
        #[arg(long, default_value = "E1_FIRST_ELIGIBLE")]
        entry: String,
        #[arg(long, default_value = "X1_TIME_2M")]
        exit: String,
        #[arg(long, default_value = "BASE")]
        latency: String,
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },
    Paper {
        #[arg(long)]
        chain: String,
    },
}

#[derive(Subcommand, Debug)]
#[allow(clippy::enum_variant_names)]
enum ResearchCmd {
    /// Write feature vectors as JSONL (point-in-time; no outcomes).
    ExportFeatures {
        #[arg(long)]
        chain: Option<String>,
        #[arg(long, default_value = "features.jsonl")]
        out: PathBuf,
        #[arg(long, default_value_t = 10000)]
        limit: i64,
    },
    ExportOutcomes {
        out: PathBuf,
    },
    ExportSimulations {
        out: PathBuf,
    },
    ExportPolicyPerformance {
        out: PathBuf,
    },
    ExportMissedWinners {
        out: PathBuf,
    },
    /// Run EXP001 if a research-valid corpus is configured; otherwise report BLOCKED.
    Exp001,
    /// Postgres connection + migrate + write/read smoke. Sanitized errors.
    DbCheck,
    /// Prospective paper/shadow collection. No broadcast. No keys.
    Prospective {
        #[arg(long, default_value = "robinhood,base")]
        chains: String,
        #[arg(long, default_value_t = 1800)]
        duration_secs: u64,
    },
    /// Locked Pons prospective paper experiment EXP001.
    PonsExp001 {
        #[command(subcommand)]
        cmd: PonsExp001Cmd,
    },
}

#[derive(Subcommand, Debug)]
enum PonsExp001Cmd {
    Preflight,
    Lock {
        #[arg(long, default_value = "PONS_PROSPECTIVE_EXP002")]
        experiment: String,
        #[arg(long, default_value_t = false)]
        relock: bool,
    },
    Start {
        #[arg(long, default_value = "PONS_PROSPECTIVE_EXP002")]
        experiment: String,
        #[arg(long)]
        duration_secs: Option<u64>,
    },
    Status {
        #[arg(long, default_value = "PONS_PROSPECTIVE_EXP002")]
        experiment: String,
    },
    Integrity {
        #[arg(long)]
        experiment: String,
    },
    Invalidate001,
}

#[derive(Subcommand, Debug)]
enum CorpusCmd {
    /// Streaming validate a normalized corpus JSONL.
    Validate {
        jsonl: PathBuf,
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
    /// Replay corpus JSONL through DecoderRegistry. Quality follows the validation gate.
    Replay {
        jsonl: PathBuf,
        #[arg(long)]
        snapshots: bool,
        #[arg(long)]
        features: bool,
        #[arg(long)]
        persist: bool,
    },
}

#[derive(Subcommand, Debug)]
enum SecurityCmd {
    Token {
        #[arg(long)]
        chain: String,
        #[arg(long)]
        token: String,
        #[arg(long)]
        launchpad: Option<String>,
        #[arg(long)]
        factory: Option<String>,
        #[arg(long)]
        bytecode: Option<PathBuf>,
        #[arg(long)]
        historical: bool,
    },
    Fixture {
        path: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    load_dotenv();
    let args = Args::parse();

    if args.print_registry {
        for factory in verified_factories() {
            println!(
                "{} {} {} {} {}",
                factory.chain,
                factory.launchpad,
                factory.address,
                factory.verification_status.as_str(),
                factory.abi_idl_version
            );
        }
        return Ok(());
    }

    match args.command {
        Some(Command::Collect { chain, mode }) => {
            let mut config = EngineConfig::from_env_with_mode(mode.as_deref());
            if let Some(raw) = mode.as_deref() {
                config.solana_mode = memecoin_engine::domain::SolanaMode::parse(raw)
                    .ok_or_else(|| anyhow::anyhow!("unknown --mode {raw}"))?;
            }
            if let Some(addr) = config.metrics_addr.as_deref() {
                memecoin_engine::metrics::install_prometheus(addr)?;
                tracing::info!(addr, "prometheus metrics listening");
            }
            let target = CollectTarget::parse(&chain).ok_or_else(|| {
                anyhow::anyhow!("unknown chain {chain}; use solana|base|robinhood|all")
            })?;
            run_collect(config, target).await?;
        }
        Some(Command::Replay {
            chain,
            fixture_dir,
            persist,
            snapshots,
            features,
        }) => {
            if chain != "solana" {
                anyhow::bail!("replay currently supports solana only");
            }
            let config = EngineConfig::from_env();
            if let Some(addr) = config.metrics_addr.as_deref() {
                memecoin_engine::metrics::install_prometheus(addr)?;
            }
            let markets = Arc::new(MarketRegistry::new());
            let report = if persist {
                let db = config.database_url.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("DATABASE_URL is required for replay --persist")
                })?;
                let store = PostgresStore::connect(db).await?;
                store.migrate().await?;
                let persisted = store
                    .load_watched_markets(memecoin_engine::domain::Chain::Solana)
                    .await?;
                markets.load_all(persisted);
                memecoin_engine::replay::replay_fixture_dir_full(
                    &fixture_dir,
                    Arc::new(store),
                    markets,
                    snapshots,
                    features,
                )
                .await?
            } else {
                memecoin_engine::replay::replay_fixture_dir_full(
                    &fixture_dir,
                    Arc::new(MemoryStore::new()),
                    markets,
                    snapshots,
                    features,
                )
                .await?
            };
            print!("{}", format_report(&report));
        }
        Some(Command::Corpus { cmd }) => match cmd {
            CorpusCmd::Validate { jsonl, manifest } => {
                let mut src = memecoin_engine::historical::PumpCorpusSource::open(&jsonl)?;
                let mut acc = memecoin_engine::historical::StreamingScan::new(true);
                while let Some(ev) =
                    memecoin_engine::historical::HistoricalSource::next_event(&mut src).await?
                {
                    acc.push(&ev);
                }
                let man = match manifest {
                    Some(p) => {
                        let t = std::fs::read_to_string(p)?;
                        Some(serde_json::from_str::<
                            memecoin_engine::historical::DatasetManifest,
                        >(&t)?)
                    }
                    None => None,
                };
                let (scan, coverage, dups, missing) = acc.finish();
                let report = memecoin_engine::historical::validate_historical_dataset(
                    man.as_ref(),
                    &scan,
                    &coverage,
                    &dups,
                    &missing,
                );
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            CorpusCmd::Replay {
                jsonl,
                snapshots,
                features,
                persist,
            } => {
                let mut src = memecoin_engine::historical::PumpCorpusSource::open(&jsonl)?;
                let mut acc = memecoin_engine::historical::StreamingScan::new(true);
                while let Some(ev) =
                    memecoin_engine::historical::HistoricalSource::next_event(&mut src).await?
                {
                    acc.push(&ev);
                }
                let (scan, coverage, dups, missing) = acc.finish();
                let validation = memecoin_engine::historical::validate_historical_dataset(
                    None, &scan, &coverage, &dups, &missing,
                );
                let mut opts = memecoin_engine::replay::ReplayOpts::corpus(
                    validation.quality_status,
                    validation.research_session_complete(),
                );
                opts.snapshots = snapshots;
                opts.features = features;
                let markets = Arc::new(MarketRegistry::new());
                let report = if persist {
                    let config = EngineConfig::from_env();
                    let db = config.database_url.as_deref().ok_or_else(|| {
                        anyhow::anyhow!("DATABASE_URL is required for corpus replay --persist")
                    })?;
                    let store = PostgresStore::connect(db).await?;
                    store.migrate().await?;
                    memecoin_engine::replay::replay_corpus_jsonl(
                        &jsonl,
                        Arc::new(store),
                        markets,
                        opts,
                    )
                    .await?
                } else {
                    memecoin_engine::replay::replay_corpus_jsonl(
                        &jsonl,
                        Arc::new(MemoryStore::new()),
                        markets,
                        opts,
                    )
                    .await?
                };
                print!("{}", format_report(&report));
                println!(
                    "dataset_verdict={} FEATURE_VALID={} EXECUTION_VALID={}",
                    validation.verdict.as_str(),
                    validation.feature_valid,
                    validation.execution_valid
                );
            }
        },
        Some(Command::Research { cmd }) => match cmd {
            ResearchCmd::ExportFeatures { chain, out, limit } => {
                let config = EngineConfig::from_env();
                let db = config.database_url.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("DATABASE_URL is required for research export-features")
                })?;
                let store = PostgresStore::connect(db).await?;
                let chain = chain
                    .as_deref()
                    .map(|c| {
                        memecoin_engine::domain::Chain::parse(c)
                            .ok_or_else(|| anyhow::anyhow!("unknown chain {c}"))
                    })
                    .transpose()?;
                let vectors = store.export_feature_vectors(chain, limit).await?;
                let file = std::fs::File::create(&out)?;
                let n = memecoin_engine::features::write_jsonl(&vectors, file)?;
                println!("wrote {n} feature vectors to {}", out.display());
            }
            ResearchCmd::ExportOutcomes { out }
            | ResearchCmd::ExportSimulations { out }
            | ResearchCmd::ExportPolicyPerformance { out }
            | ResearchCmd::ExportMissedWinners { out } => {
                println!(
                    "wrote research export placeholder to {} (run `simulate historical` for a populated JSONL; outcomes are never mixed into feature export)",
                    out.display()
                );
                std::fs::write(
                    &out,
                    "{\"note\":\"outcomes/simulations live in simulation_runs and token_outcomes, not feature_vectors\"}\n",
                )?;
            }
            ResearchCmd::DbCheck => {
                let url = std::env::var("DATABASE_URL").unwrap_or_default();
                if url.is_empty() {
                    println!("BLOCKED_DATABASE: DATABASE_URL is not set");
                    std::process::exit(2);
                }
                let report = memecoin_engine::storage::dbcheck::check_database(&url).await;
                println!("{}", serde_json::to_string_pretty(&report)?);
                if report.blocked {
                    std::process::exit(2);
                }
            }
            ResearchCmd::PonsExp001 { cmd } => {
                let url = std::env::var("DATABASE_URL").unwrap_or_default();
                if url.is_empty() {
                    println!("BLOCKED_DATABASE: DATABASE_URL is not set");
                    std::process::exit(2);
                }
                match cmd {
                    PonsExp001Cmd::Preflight => {
                        let config = EngineConfig::from_env();
                        match memecoin_engine::lab::pons_run::cmd_preflight(config).await {
                            Ok(r) => {
                                println!("{}", serde_json::to_string_pretty(&r)?);
                                if r.verdict != "PREFLIGHT_PASS" {
                                    std::process::exit(2);
                                }
                            }
                            Err(e) => {
                                println!("preflight error: {e}");
                                std::process::exit(2);
                            }
                        }
                    }
                    PonsExp001Cmd::Lock { experiment, relock } => {
                        let r = if relock {
                            memecoin_engine::lab::pons_run::cmd_relock_id(&url, &experiment).await
                        } else {
                            memecoin_engine::lab::pons_run::cmd_lock_id(&url, &experiment).await
                        };
                        match r {
                            Ok(st) => println!(
                                "LOCKED experiment_id={} config_hash={} status={}",
                                st.lock.experiment_id,
                                st.config_hash,
                                st.run_status.as_str()
                            ),
                            Err(e) => {
                                println!("lock error: {e}");
                                std::process::exit(2);
                            }
                        }
                    }
                    PonsExp001Cmd::Start {
                        experiment,
                        duration_secs,
                    } => {
                        println!("{experiment} start (paper only, no keys, no broadcast)");
                        let config = EngineConfig::from_env();
                        let dur = duration_secs.map(std::time::Duration::from_secs);
                        if let Err(e) = memecoin_engine::lab::pons_run::cmd_start_id_for(
                            config,
                            &experiment,
                            dur,
                        )
                        .await
                        {
                            println!("start error: {e}");
                            std::process::exit(2);
                        }
                    }
                    PonsExp001Cmd::Status { experiment } => {
                        match memecoin_engine::lab::pons_run::cmd_status_id(&url, &experiment).await
                        {
                            Ok(r) => println!("{}", serde_json::to_string_pretty(&r)?),
                            Err(e) => {
                                println!("status error: {e}");
                                std::process::exit(2);
                            }
                        }
                    }
                    PonsExp001Cmd::Integrity { experiment } => {
                        match memecoin_engine::lab::pons_run::cmd_integrity(&url, &experiment).await
                        {
                            Ok(r) => {
                                println!("{}", serde_json::to_string_pretty(&r)?);
                                if !r.ok {
                                    std::process::exit(2);
                                }
                            }
                            Err(e) => {
                                println!("integrity error: {e}");
                                std::process::exit(2);
                            }
                        }
                    }
                    PonsExp001Cmd::Invalidate001 => {
                        match memecoin_engine::lab::pons_run::cmd_invalidate_exp001(&url).await {
                            Ok(st) => println!(
                                "INVALIDATED {} reason={}",
                                st.lock.experiment_id,
                                st.pause_reason.unwrap_or_default()
                            ),
                            Err(e) => {
                                println!("invalidate error: {e}");
                                std::process::exit(2);
                            }
                        }
                    }
                }
            }
            ResearchCmd::Prospective {
                chains,
                duration_secs,
            } => {
                let url = std::env::var("DATABASE_URL").unwrap_or_default();
                if url.is_empty() {
                    println!("BLOCKED_DATABASE: DATABASE_URL is not set");
                    std::process::exit(2);
                }
                let report = memecoin_engine::storage::dbcheck::check_database(&url).await;
                if report.blocked {
                    println!("{}", report.message);
                    std::process::exit(2);
                }
                println!(
                    "PROSPECTIVE_PAPER db=ok chains={chains} duration_secs={duration_secs} (no broadcast, no keys)"
                );
                let collect_target =
                    CollectTarget::parse(&chains.replace(' ', "")).unwrap_or_else(|| {
                        if chains.contains("base") && chains.contains("robinhood") {
                            CollectTarget::Evm
                        } else if chains.contains("base") {
                            CollectTarget::Base
                        } else {
                            CollectTarget::Robinhood
                        }
                    });
                let config = EngineConfig::from_env();
                let total = duration_secs.max(1);
                let first = (total / 2).max(1);
                let second = total.saturating_sub(first).max(1);
                println!("prospective phase A {first}s then restart recovery {second}s");
                let opts_a = memecoin_engine::collect::CollectOpts {
                    paper: true,
                    duration: Some(std::time::Duration::from_secs(first)),
                    ..Default::default()
                };
                if let Err(e) = memecoin_engine::collect::run_collect_opts(
                    config.clone(),
                    collect_target,
                    opts_a,
                )
                .await
                {
                    println!("prospective phase A error: {e}");
                } else {
                    println!("phase A ended; restarting collectors (postgres position reload)");
                }
                let opts_b = memecoin_engine::collect::CollectOpts {
                    paper: true,
                    duration: Some(std::time::Duration::from_secs(second)),
                    ..Default::default()
                };
                match memecoin_engine::collect::run_collect_opts(config, collect_target, opts_b)
                    .await
                {
                    Ok(()) => println!(
                        "prospective collect ended after {duration_secs}s including restart; open positions SESSION_ENDED_OPEN"
                    ),
                    Err(e) => println!("prospective phase B error: {e}"),
                }
            }
            ResearchCmd::Exp001 => {
                let jsonl = std::env::var("MEMECOIN_CORPUS_JSONL").ok();
                let dir = std::env::var("MEMECOIN_HISTORICAL_DIR").ok();
                let path = jsonl.as_deref().map(std::path::PathBuf::from).or_else(|| {
                    dir.as_deref().map(|d| {
                        std::path::Path::new(d)
                            .join("normalized")
                            .join("corpus.jsonl")
                    })
                });
                match path {
                    Some(p) if p.is_file() => {
                        let mut src = memecoin_engine::historical::PumpCorpusSource::open(&p)?;
                        let mut acc = memecoin_engine::historical::StreamingScan::new(true);
                        while let Some(ev) =
                            memecoin_engine::historical::HistoricalSource::next_event(&mut src)
                                .await?
                        {
                            acc.push(&ev);
                        }
                        let (scan, coverage, dups, missing) = acc.finish();
                        let report = memecoin_engine::historical::validate_historical_dataset(
                            None, &scan, &coverage, &dups, &missing,
                        );
                        let verdict = memecoin_engine::lab::exp001::exp001_verdict(&report);
                        println!(
                            "{} FEATURE_VALID={} EXECUTION_VALID={} dataset={} quality={}",
                            verdict.as_str(),
                            report.feature_valid,
                            report.execution_valid,
                            report.verdict.as_str(),
                            report.quality_status.as_str()
                        );
                        if memecoin_engine::lab::exp001::exp001_may_run_test(&report).is_err() {
                            println!(
                                "EXP001_BLOCKED_DATASET: strategy TEST not run. See research/EXP001_DATASET_REPORT.md"
                            );
                        }
                    }
                    _ => {
                        println!(
                            "EXP001_BLOCKED_DATASET: set MEMECOIN_CORPUS_JSONL or MEMECOIN_HISTORICAL_DIR to a validated Pump.fun corpus. Fixtures alone are not a research-valid strategy sample."
                        );
                    }
                }
            }
        },
        Some(Command::Simulate { cmd }) => match cmd {
            SimulateCmd::Historical {
                fixture_dir,
                entry,
                exit,
                latency,
                seed,
            } => {
                let markets = Arc::new(MarketRegistry::new());
                let report = memecoin_engine::replay::replay_fixture_dir_full(
                    &fixture_dir,
                    Arc::new(MemoryStore::new()),
                    markets,
                    true,
                    false,
                )
                .await?;
                let entry = memecoin_engine::sim::EntryPolicyId::parse(&entry)
                    .ok_or_else(|| anyhow::anyhow!("unknown entry policy"))?;
                let xpol = memecoin_engine::sim::exit_policy(&exit);
                let lat = memecoin_engine::sim::LatencyScenario::parse(&latency)
                    .ok_or_else(|| anyhow::anyhow!("unknown latency"))?;
                let cfg = memecoin_engine::sim::SimConfig::research_default().with_latency(lat);
                let mut eligible = std::collections::HashMap::new();
                let mut cand = Vec::new();
                let mut sec = Vec::new();
                for s in &report.snapshots {
                    if s.age_ms >= 15_000 {
                        eligible
                            .entry((s.chain, s.token_address.clone()))
                            .or_insert(s.snapshot_time);
                    }
                    cand.push((
                        s.snapshot_time,
                        s.chain,
                        s.token_address.clone(),
                        memecoin_engine::candidate::CandidateState::Eligible,
                    ));
                    sec.push((
                        s.snapshot_time,
                        s.chain,
                        s.token_address.clone(),
                        memecoin_engine::security::assessment::SecurityVerdict::Pass,
                    ));
                }
                let sim = memecoin_engine::sim::run_historical(
                    &report.snapshots,
                    eligible,
                    &sec,
                    &cand,
                    entry,
                    xpol.as_ref(),
                    &cfg,
                    memecoin_engine::domain::QualityStatus::HistoricalReplay,
                    seed,
                );
                let perf = memecoin_engine::sim::policy_performance(&sim);
                println!(
                    "historical simulation policy={} orders={} fills={} closed={} research_valid={} sample_insufficient={}",
                    perf.policy_id,
                    perf.n_orders,
                    perf.filled_entries,
                    perf.trades_closed,
                    perf.research_valid,
                    perf.sample_insufficient
                );
            }
            SimulateCmd::Paper { chain } => {
                let _ = memecoin_engine::domain::Chain::parse(&chain);
                println!(
                    "PaperExecutionEngine: simulated fills from live snapshots only. No broadcast, no keys. {}",
                    memecoin_engine::sim::LiveExecutionEngine::not_implemented()
                );
                println!("LIVE RH/BASE PAPER VALIDATION: NEEDS_VERIFICATION (requires LIVE_COMPLETE session)");
            }
        },
        Some(Command::Security { cmd }) => match cmd {
            SecurityCmd::Token {
                chain,
                token,
                launchpad,
                factory,
                bytecode,
                historical,
            } => {
                use memecoin_engine::domain::{Chain, Launchpad, QualityStatus, TokenDiscovered};
                use memecoin_engine::security::context::{EvmView, SecurityContext};
                use memecoin_engine::security::{format_assessment, SecurityEngine};
                let chain = Chain::parse(&chain).ok_or_else(|| anyhow::anyhow!("bad chain"))?;
                let lp = launchpad
                    .as_deref()
                    .map(Launchpad::parse)
                    .unwrap_or(Launchpad::Unknown);
                let mut tok = TokenDiscovered {
                    chain,
                    chain_id: chain.evm_chain_id(),
                    token_address: token,
                    creator: String::new(),
                    launchpad: lp,
                    factory_or_program: factory.unwrap_or_default(),
                    pool: None,
                    curve: None,
                    quote_asset: None,
                    launch_mechanism: memecoin_engine::domain::LaunchMechanism::Unknown,
                    bonding_curve: false,
                    graduation_model: memecoin_engine::domain::GraduationModel::Unknown,
                    block_number: None,
                    block_hash: None,
                    slot: None,
                    tx_hash_or_signature: String::new(),
                    instruction_index: None,
                    inner_instruction_index: None,
                    log_index: None,
                    chain_timestamp: None,
                    observed_at: chrono::Utc::now(),
                    persisted_at: None,
                    source: "cli".into(),
                    decoder_version: String::new(),
                    initial_liquidity: None,
                    raw_event_id: "cli".into(),
                };
                if tok.factory_or_program.is_empty() {
                    tok.factory_or_program = match lp {
                        Launchpad::PumpFun => memecoin_engine::registry::PUMPFUN_PROGRAM.into(),
                        Launchpad::PonsV2 => memecoin_engine::registry::PONS_V2_FACTORY.into(),
                        Launchpad::ClankerV4 => {
                            memecoin_engine::registry::CLANKER_V4_FACTORY.into()
                        }
                        _ => String::new(),
                    };
                }
                let mut ctx = SecurityContext::from_token(
                    tok,
                    QualityStatus::DevelopmentIncomplete,
                    historical,
                );
                if let Some(p) = bytecode {
                    let bytes = std::fs::read(&p)?;
                    let hexed = if bytes.first() == Some(&b'0') {
                        hex::decode(std::str::from_utf8(&bytes)?.trim().trim_start_matches("0x"))?
                    } else {
                        bytes
                    };
                    ctx.evm = Some(EvmView {
                        runtime_bytecode: Some(hexed),
                        as_of_requested_block: !historical,
                        ..Default::default()
                    });
                }
                let a = SecurityEngine::default().assess(&ctx);
                print!("{}", format_assessment(&a));
            }
            SecurityCmd::Fixture { path } => {
                use memecoin_engine::decoders::DecoderRegistry;
                use memecoin_engine::domain::QualityStatus;
                use memecoin_engine::security::context::SecurityContext;
                use memecoin_engine::security::{format_assessment, SecurityEngine};
                let rel = path.to_string_lossy().to_string();
                let raw = if rel.contains("solana") {
                    memecoin_engine::test_support::solana_raw_from_fixture(
                        rel.trim_start_matches("tests/fixtures/"),
                        "create_instruction_index",
                    )
                } else {
                    memecoin_engine::test_support::evm_raw_from_fixture(
                        rel.trim_start_matches("tests/fixtures/"),
                    )
                };
                let tok = DecoderRegistry::production()
                    .decode(&raw)?
                    .into_token()
                    .ok_or_else(|| anyhow::anyhow!("fixture did not decode a token"))?;
                let ctx = SecurityContext::from_token(tok, QualityStatus::HistoricalReplay, true);
                let a = SecurityEngine::default().assess(&ctx);
                print!("{}", format_assessment(&a));
            }
        },
        None => {
            tracing::info!(
                "phase 2.1B engine idle. collect solana --mode rpc-dev|yellowstone, replay solana <dir>, collect base|robinhood."
            );
        }
    }
    Ok(())
}
