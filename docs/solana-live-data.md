# Solana live data

**FREE MODE IS NOT COMPLETE LIVE SOLANA.**

Yellowstone gRPC is implemented and remains the production/research-grade path. It is **not activated**. We are not paying for Alchemy Yellowstone, Helius Business, Triton, or any other Solana gRPC provider during research/development.

## Current

| Mode | Status | Completeness |
|---|---|---|
| Historical / fixture | **SUPPORTED** | `HISTORICAL_REPLAY` — complete for the window the files cover |
| Free RPC development (`rpc_dev`) | **SUPPORTED BUT INCOMPLETE** | `RPC_DEV_INCOMPLETE` / `DEVELOPMENT_INCOMPLETE` |
| Research-grade Yellowstone | **IMPLEMENTED BUT NOT ACTIVATED** | `LIVE_COMPLETE` only after explicit enable + a real Geyser endpoint |

```
SOLANA
    historical corpus
    real lifecycle fixtures
    free RPC for development
    offline replay
    state/simulation development

BASE
    live

ROBINHOOD
    live
```

Solana remains the primary **historical** strategy-research chain. Robinhood and Base provide live collection.

## Why Yellowstone is deferred

Phase 2.1 connected to a configured Alchemy HTTP host. That host did not expose `/geyser.Geyser/Subscribe`. Paying for a Geyser SKU is not justified until we need complete live Solana for paper trading or prospective research.

The JSON-RPC path (`logsSubscribe` → `getTransaction`) is **development only**. It must never be used for production paper-performance metrics. Strategy research must reject `rpc_dev` sessions via `validate_dataset_quality()`.

## Modes

Set `SOLANA_MODE` or `memecoin-engine collect solana --mode …`.

| Value | Meaning |
|---|---|
| `historical` | No live provider. Use `replay solana <fixture-dir>` (fixtures, stored txs, replay files). |
| `rpc_dev` | Free/public RPC + WSS. Incomplete. Warns once at startup. Session `complete=false`. |
| `yellowstone` | Research-grade Geyser. **Required to connect**, even if credentials are present. |

Default (no flag, no env): `rpc_dev`. Credentials in `SOLANA_GRPC_URL` **do not** auto-enable Yellowstone (cost guard).

### Activate Yellowstone later (config, not a rewrite)

```bash
SOLANA_MODE=yellowstone
SOLANA_GRPC_URL=...          # any Yellowstone-compatible host
SOLANA_GRPC_TOKEN=...
memecoin-engine collect solana --mode yellowstone
```

Provider layer (`ingest/solana/provider.rs`) holds URL, auth, and transport quirks only. Pump decoding does not branch on Alchemy vs Helius vs Triton vs other Geyser hosts.

Trigger to pay: serious live Solana paper trading, or when complete real-time Solana research data becomes necessary.

## Architecture (unchanged)

```
Yellowstone  →  RawEvent  →  DecoderRegistry  →  canonical events
```

`logsSubscribe` is not the architectural primary path.

## Session quality

Every collection/replay writes `collection_sessions` with `source_mode`, `provider`, `completeness_status` (`quality_status`), `started_at`, `ended_at`.

| quality_status | complete |
|---|---|
| `HISTORICAL_REPLAY` | true (for the covered window) |
| `RPC_DEV_INCOMPLETE` | **false** (never proven complete) |
| `LIVE_COMPLETE` | true only for explicit Yellowstone / EVM live when gaps recovered |

Later simulation:

```text
if session.chain == SOLANA
and session.mode == rpc_dev
and simulation_requires_complete_market_data
then DatasetQualityError::IncompleteSource
```

## rpc_dev warning (once)

```
WARNING:
Solana rpc_dev mode is incomplete and must not be used for strategy performance evaluation.
```
