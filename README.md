# Memecoin bot

Research: `MEMECOIN_BOT_RESEARCH.md`, `MEMECOIN_BOT_RESEARCH_V2.md`.

Phase 7 is **strategy research + moonshot capture**. There is no live trading. `ELIGIBLE` ≠ BUY. EXP001 requires a research-valid corpus.

**FREE MODE != COMPLETE LIVE SOLANA.** Yellowstone is implemented but not activated. Do not pay for Solana gRPC yet. See `docs/solana-live-data.md`.

## Layout

- `apps/engine` — Rust collector + discovery engine + offline replay
- `crates/programs` — pinned Pump.fun IDL, Pons V2 ABI, Clanker v4 ABI, Uniswap v4 events
- `sql/migrations` — Postgres schema (`collection_sessions` in 0004)
- `tests/fixtures` — real on-chain events
- `docs/strategy-research.md`, `research/EXP001_REPORT.md`

## Setup

```bash
cp .env.example .env
# fill RPC/WS URLs (never commit secrets)
docker compose up -d postgres
```

Apply migrations by running the engine (it migrates on collect/replay --persist) or:

```bash
cargo test -p memecoin-engine postgres_migrations_apply_and_are_idempotent
```

## Configure providers

| Variable | Purpose |
|---|---|
| `DATABASE_URL` | Postgres |
| `SOLANA_MODE` | `historical` \| `rpc_dev` (default) \| `yellowstone` |
| `SOLANA_GRPC_URL` / `SOLANA_GRPC_TOKEN` | Yellowstone gRPC. **Ignored unless `SOLANA_MODE=yellowstone`** (cost guard) |
| `SOLANA_RPC_URL` / `SOLANA_WS_URL` | `rpc_dev` collect + Yellowstone repair |
| `BASE_WS_URL` / `BASE_HTTP_URL` | Base `eth_subscribe` + `eth_getLogs` |
| `ROBINHOOD_WS_URL` / `ROBINHOOD_HTTP_URL` | Robinhood Chain |
| `METRICS_ADDR` | Prometheus scrape (e.g. `127.0.0.1:9100`) |

HTTP URLs are derived from `wss://` → `https://` when omitted. API keys in URLs are redacted in logs.

## Run collectors

```bash
cargo run -p memecoin-engine -- collect solana --mode rpc-dev
cargo run -p memecoin-engine -- replay solana tests/fixtures/solana/lifecycle --snapshots
cargo run -p memecoin-engine -- replay solana tests/fixtures/solana/lifecycle --features
cargo run -p memecoin-engine -- collect base
cargo run -p memecoin-engine -- collect robinhood
cargo run -p memecoin-engine -- collect all --mode rpc-dev
```

`rpc_dev` prints a one-time warning: it is incomplete and must not be used for strategy performance evaluation.

Yellowstone (later, paid):

```bash
SOLANA_MODE=yellowstone SOLANA_GRPC_URL=... SOLANA_GRPC_TOKEN=... \
  cargo run -p memecoin-engine -- collect solana --mode yellowstone
```

SIGINT / SIGTERM stops the stream, flushes the pipeline, and exits.

## Inspect

```bash
# recently discovered tokens
psql "$DATABASE_URL" -c 'select chain, token_address, launchpad, created_at from tokens order by created_at desc limit 20;'

# trades
psql "$DATABASE_URL" -c 'select chain, token_address, side, base_amount_raw, observed_at from token_trades order by observed_at desc limit 20;'

# lifecycle
psql "$DATABASE_URL" -c "select type, token_address, block_number, chain_time from lifecycle_events order by chain_time desc limit 20;"

# session quality (reject RPC_DEV_INCOMPLETE in research)
psql "$DATABASE_URL" -c 'select chain, mode, quality_status, complete, provider, started_at from collection_sessions order by started_at desc limit 20;'

# metrics
curl -s http://127.0.0.1:9100/metrics | head
```

## Tests

Offline (no RPC):

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Stop

Ctrl-C the engine process. `docker compose stop postgres` stops the database.
