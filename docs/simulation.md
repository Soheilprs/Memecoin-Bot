# Simulation (Phase 6)

No broadcast. No private keys. `LiveExecutionEngine` is a stub.

```
strategy policy
  → ExecutionEngine (Historical | Paper)
  → PositionManager
  → OutcomeEngine
```

`ELIGIBLE` still does not mean buy. A Phase 6 **entry policy** may submit an `EntryRequest` only after security PASS/WARN and candidate ELIGIBLE.

## Historical vs paper

| | Historical | Paper |
|---|---|---|
| Clock | `ReplayClock` / logical snapshot time | `LiveClock` (`as_of = now`) |
| Fill | snapshot at `decision + delay` in the dataset | snapshot at `decision + delay` **only if that time has already arrived** |
| Post-hoc | forbidden | forbidden: future live ticks cannot fill a past decision |

If no executable snapshot exists at fill time: `NO_FILL`.

## Research-valid

`RPC_DEV_INCOMPLETE` → `research_valid = false`. Those runs must not enter strategy-performance reports.

Accepted sources: `HISTORICAL_REPLAY`, `LIVE_COMPLETE`.

## CLI

```bash
cargo run -p memecoin-engine -- simulate historical tests/fixtures/solana/lifecycle --entry E1_FIRST_ELIGIBLE --exit X1_TIME_2M --latency BASE
cargo run -p memecoin-engine -- simulate paper --chain robinhood
```

Paper CLI does not send transactions.

See `docs/execution-models.md`, `docs/outcomes.md`, `docs/moonshot-analysis.md`.
