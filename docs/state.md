# Continuous token state (Phase 3)

Canonical events become a point-in-time view of each token. There is still **no trading, scoring, ML, or simulation**.

```
Canonical events (DecoderRegistry)
        │
        ▼
   StateEngine  (same for live and replay)
        │
        ├── TokenState (HOT in RAM)
        ├── MarketState (curve / CP / Uniswap v4)
        └── RollingFlowState (5s … 15m)
        │
        ▼
token_state_snapshots   (append-only history)
token_current_state     (cache only)
```

A snapshot at time T includes **only** events with chain order / event time `<= T`. Replay uses `ReplayClock` (logical market time), never wall-clock speed.

## Lifecycle (protocol-specific)

| Protocol | Path |
|---|---|
| Pump.fun | `DISCOVERED` → `CURVE_ACTIVE` → `MIGRATING` → `AMM_ACTIVE` (PumpSwap) |
| Pons V2 | `DISCOVERED` → `CURVE_ACTIVE` → `LAUNCH_SWEPT` / `GRADUATION_GAP` → `AMM_ACTIVE` |
| Clanker v4 | `DISCOVERED` → `AMM_ACTIVE` immediately |

`GRADUATION_GAP` is **not** ordinary AMM trading. Later simulation must treat it as possibly unsellable.

`REJECTED_SECURITY` is reserved for Phase 4; sparse outcome snapshots can still be stored.

## Amounts

Volumes and reserves are decimal integer strings (`U256`). No `f64` source of truth. Quote assets stay native (SOL/ETH/WETH/USDG/…). No USD conversion.

Pump curve progress is `curve_progress_bps` (0–10000) from first observed `virtual_token_reserves` vs current. PumpSwap vault reserves are **not** fabricated: `market_state_quality = PARTIAL` until Yellowstone account subscriptions exist.

## Snapshots

Milestones from discovery: T+5s, 15s, 30s, 60s, 2m, 5m, 15m, 30m, 1h.

Periodic (configurable): 0–5m / 5s, 5–30m / 15s, 30–120m / 60s.

Lifecycle snapshots: `TOKEN_CREATED`, `FIRST_TRADE`, `MIGRATED`, `LAUNCH_SWEPT`, `POOL_GRADUATED`, `AMM_FIRST_TRADE`, …

Each row stores `as_of_event_id`, `as_of_block`/`slot`, `as_of_event_order`, `source_session_id`, `data_quality`, `fingerprint`. Reorg rebuild marks prior rows `superseded=true` and writes a new version. Research queries default to `NOT superseded`.

`snapshot_time` is logical market time. `created_at` is DB write time.

Dead tokens still receive zero-activity milestones (no survivorship bias).

## Quality

Source quality **never upgrades** because a snapshot was produced.

| Source | Snapshot `data_quality` | Complete-data simulation |
|---|---|---|
| historical/fixture | `HISTORICAL_REPLAY` | accepted |
| `rpc_dev` | `RPC_DEV_INCOMPLETE` | **rejected** |
| live RH/Base | `LIVE_COMPLETE` | accepted when session complete |
| PumpSwap without reserves | still source quality; market JSON `PARTIAL` | — |

`validate_snapshot_for_simulation()` is the Phase 4–7 guard.

Phase 5 consumes snapshots plus security assessments: see `docs/features.md` and `docs/candidate-engine.md`.

## Replay

```bash
memecoin-engine replay solana tests/fixtures/solana/lifecycle --snapshots
```

## Memory

HOT (default 30m): full rolling windows. WARM (to 2h): totals only. COLD: evict RAM. Canonical events and snapshots stay in Postgres.
