# Discovery and live collection

## Data flow

```
LIVE / HISTORICAL SOURCES
  Solana
    historical  — fixtures / jsonl / future corpus  (complete for the window)
    rpc_dev     — free JSON-RPC  (INCOMPLETE, never research-grade)
    yellowstone — Geyser gRPC    (implemented, not activated; paid later)
  Robinhood / Base — Alloy-compatible eth_subscribe logs + eth_getLogs (live)
        │
        ▼
     RawEvent  (Tokio bounded mpsc)
        │
        ▼
  DecoderRegistry   ← same registry for live, historical, and fixture replay
        │
        ├── TokenDiscovered
        ├── TradeObserved
        └── LifecycleObserved
        │
        ▼
     Postgres (system of record, not the message bus)
     collection_sessions.quality_status
        │
        ▼
   StateEngine (live and replay share one implementation)
        │
        ▼
     token_state_snapshots (append-only, point-in-time)
```

See `docs/state.md`. Snapshots at T never include events after T.

**FREE MODE != COMPLETE LIVE SOLANA.** `rpc_dev` sessions are stored as `RPC_DEV_INCOMPLETE` / `complete=false`. Call `validate_dataset_quality()` before simulation. See `docs/solana-live-data.md` and `docs/historical-replay.md`.

Postgres is the **system of record**. It is **not** the in-process message bus.

## Phase 2 collectors

| Adapter | Transport | What it collects |
|---|---|---|
| Solana historical | Fixtures / JSONL via `memecoin-engine replay solana <dir>` | Same Pump.fun / PumpSwap events, offline |
| Solana `rpc_dev` | Free JSON-RPC `logsSubscribe` + `getTransaction` | Development only. Incomplete. Not for paper-performance metrics |
| Solana `yellowstone` | Yellowstone gRPC (`SOLANA_MODE=yellowstone` **and** `SOLANA_GRPC_URL` + `SOLANA_GRPC_TOKEN`). Credentials alone do not connect. | Pump.fun create/buy/sell/migrate; PumpSwap create_pool/buy/sell for **watched** pools |
| Robinhood | `eth_subscribe("logs")` + `eth_getLogs` | Pons V2 factory `TokenLaunched`, `LaunchSwept`, `PoolGraduated`; curve `CurveBuy`/`CurveSell`/`SnipeTaxCharged`/`CurveCompleted` by **topic0** (no address filter); Uniswap v4 `Initialize`/`Swap` on the verified PoolManager, filtered to watched Pons pools |
| Base | same EVM transport | Clanker v4 `TokenCreated`; Uniswap v4 `Initialize`/`Swap` on the verified PoolManager, filtered to discovered Clanker `poolId`s |

### Pons curve architecture

Each Pons token has its own curve contract. Phase 2 **subscribes to CurveBuy/CurveSell topic0s globally** (no address filter) rather than dynamically adding addresses per `TokenLaunched`.

Reasons: correct (new curves are collected immediately), maintainable (no resubscribe), cost-efficient on Robinhood vs processing all logs. Factory events still match the verified factory address in the decoder.

`LaunchSwept` and `PoolGraduated` are stored as **separate** lifecycle types. They are never collapsed.

### PumpSwap

`migrate` / `CompletePumpAmmMigrationEvent` records the PumpSwap pool. A PumpSwap `create_pool` decoder exists. **Live swap tracking for the entire PumpSwap program is not enabled** (unreasonable load). Dynamic per-pool gRPC account filters are an unresolved Phase 2 item.

### Clanker / Uni v4 pool matching

`TokenCreated.poolId` is the Uniswap v4 `PoolId`. Swaps are matched by `Swap` topic1 == `poolId`. Unwatched PoolManager swaps are held until the matching `TokenCreated` arrives (same-tx case) then dropped if still unknown.

## Finality and reorgs

EVM `removed=true` **does not delete**. Rows become `canonical_status=orphaned`.

Solana finality is recorded on the raw event. `observed_at` never changes when finality changes.

## Resume / backfill

Checkpoints store last block/slot plus overlap. On reconnect:

1. load checkpoint
2. rewind overlap
3. `eth_getLogs` / signature replay
4. deduplicate by event id
5. resume live
6. if the provider cannot fill the range, write `ingest_gaps` and leave `recovered=false`

## Timestamps

| Field | Meaning |
|---|---|
| `chain_time` | When the chain produced the event |
| `observed_at` | When this process first received it (not batch flush) |
| `persisted_at` | When the database write completed |

Amounts are **decimal integer strings** (raw units + decimals). Never `f64`.

## Event ordering

EVM: `block_number`, `transaction_index`, `log_index`.

Solana: `slot`, `transaction_index` when the transport provides it (Yellowstone), otherwise `signature` + `instruction_index` + `inner_instruction_index`. JSON-RPC `getTransaction` does not include a block transaction index; that limitation is explicit.

## Run

```bash
memecoin-engine collect solana --mode rpc-dev
memecoin-engine collect solana --mode yellowstone   # paid Geyser; explicit only
memecoin-engine replay solana tests/fixtures/solana/lifecycle
memecoin-engine collect base
memecoin-engine collect robinhood
memecoin-engine collect all --mode rpc-dev
```
