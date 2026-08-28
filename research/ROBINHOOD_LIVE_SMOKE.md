# Robinhood live smoke

**Label: LIVE_SMOKE.** Not edge proof. No win rate.

## Database

`research db-check`: connected, migrated, write/read smoke **ok**.

Host port **5435** (5432/5433/5434 already taken on this machine). Compose maps `5435:5432`. Credentials: local docker `memecoin` / `memecoin` (see `.env.example`).

## Session

| | |
|---|---|
| Duration | 90 seconds (software path). Command for 30 min: `memecoin-engine research prospective --chains robinhood,base --duration-secs 1800` |
| Chain | robinhood |
| Collector | live websocket + HTTP (Alchemy) |
| quality_status | LIVE_COMPLETE at session open (optimistic; 90s is PARTIAL coverage) |

Observed persistence (no addresses printed):

| | N |
|---|---|
| tokens | 66 |
| token_discovered | 16 |
| trades | 372 |
| snapshots | included in 397 total (with Base) |

Feature vectors in this window: **0** (collect loop persists state; FeatureEngine/paper tick is covered by tests, not yet on the 15s live interval).

Paper orders / fills / positions: **0** in this window. Pons snipe-window skip, graduation-gap unsellable, and position restart are unit-tested.

## Data quality groups

| Group | Status |
|---|---|
| discovery | PARTIAL (90s) |
| trades | PARTIAL |
| state | PARTIAL (snapshots persisted) |
| security | PARTIAL (queue started) |
| features | INCOMPLETE in this window |
| execution | INCOMPLETE in this window |
| outcomes | INCOMPLETE in this window |

Live vs backfill intersection: not computed in 90s. Phase 2 overlap/dedup tests still apply.

## Restart

Unit test: open paper position → mark `SESSION_ENDED_OPEN` → token remains in open set → no duplicate entry.

## Continue

```bash
memecoin-engine research db-check
memecoin-engine research prospective --chains robinhood --duration-secs 1800
```
