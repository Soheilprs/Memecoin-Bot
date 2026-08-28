# Robinhood prospective smoke (Phase 7.3)

**Label: LIVE_FEATURE_PASS / paper path attempted. Not edge. No headline PnL.**

`research_valid_for_alpha = false` for PIPELINE_SMOKE_POLICY and for P0–P4 in this window (PRELIMINARY, SMALL_SAMPLE).

## Session

| | |
|---|---|
| Start | 2026-08-27 17:35:05 UTC |
| Restart | 2026-08-27 17:50:43 UTC (phase A → B, Postgres hydrate 222 → 512 tokens) |
| End | 2026-08-27 18:05:45 UTC |
| Wall duration | 1844 s (~30.7 min) including planned restart |
| RH raw blocks | 47625122 – 47643291 |
| Base raw blocks | 50531421 – 50532246 |
| Reconnects in-session | 0 (only the planned stop/start) |

## Counts (Robinhood unless noted)

| Item | N |
|---|---|
| TokenLaunched | 570 |
| CurveBuy | 6205 |
| CurveSell | 5273 |
| SnipeTaxCharged | 372 |
| LaunchSwept | 2 |
| PoolGraduated | 2 |
| tokens discovered | 570 |
| raw events | 12797 |
| trades | 11478 |
| snapshots | 95737 |
| feature vectors | 94622 |
| security assessments | 570 (all WARN; coverage 100%; UNKNOWN=0) |
| candidate transitions | 2952 (ELIGIBLE 249) |
| paper orders (smoke) | 1038 |
| fills / partial fills | 0 / 0 |
| failed / unavailable | 1038 (`UNKNOWN_CURVE_RESERVES` 1037, `PONS_GRADUATION_GAP` 1) |
| positions opened / closed / still open | 0 / 0 / 0 |
| descriptive outcomes | 5532 rows (172 reached_2x, 21 reached_5x) |

## Feature milestone coverage

Denominator = tokens discovered in this session that were old enough at session end.

| Horizon | Eligible | Vectors | Coverage |
|---|---|---|---|
| T+30 | 555 | 545 | 98.2% |
| T+60 | 545 | 540 | 99.1% |
| T+2m | 535 | 518 | 96.8% |
| T+5m | 438 | 434 | 99.1% |

Phase 7.2 had **0** live feature vectors because collect persisted snapshots on events only and never ticked FeatureEngine. Phase 7.3 uses a centralized milestone heap + 250 ms `live_tick_once` with the same FeatureEngine as replay.

## Live vs backfill

Alchemy free `eth_getLogs` is capped at a 10-block span, so the full 18k-block session was not re-fetched. First and last 80 session blocks:

| Window | Type | live | backfill | intersection | match % |
|---|---|---|---|---|---|
| start 47625122–47625201 | TokenLaunched | 2 | 2 | 2 | 100 |
| start | CurveBuy | 40 | 57 | 40 | 70.2 |
| start | CurveSell | 43 | 55 | 43 | 78.2 |
| end 47643212–47643291 | TokenLaunched | 6 | 6 | 6 | 100 |
| end | CurveBuy | 49 | 49 | 49 | 100 |
| end | CurveSell | 25 | 25 | 25 | 100 |
| end | SnipeTaxCharged | 1 | 1 | 1 | 100 |

`live_only = 0` in every sampled class. Start-window `backfill_only` buys/sells are trades on curves not yet in the watch set (session open). End-window match is 100% after canonical identity `(tx_hash, log_index)`.

Stale checkpoint skip at process start is recorded as unrecovered **prior-session** gap, not an in-window ingest hole. Collection sessions themselves have `gap_count=0` and `LIVE_COMPLETE`.

## Paper timeline (one in-session token)

Token `0x61e99e95…c72c1` (truncated; no user info):

1. **Discover** 17:35:17 UTC block 47625168 tx `0x54a0e92104437bc7…`
2. **Security** WARN at 17:35:17
3. **T+5 / T+15 / T+30 / T+60 / T+2m / T+5m** feature vectors at 17:35:22 … 17:40:17
4. **Candidate** WATCHING at 17:35:32 → EXPIRED at 17:40:17
5. **Paper decision** PIPELINE_SMOKE_POLICY at 17:35:22
6. **Fill** `UNAVAILABLE_MARKET_STATE` / `UNKNOWN_CURVE_RESERVES` (Pons CurveBuy logs do not carry virtual reserves; committed ABI is events-only; no invented x*y=k)

## Restart recovery

Phase B hydrated **512** tokens from Postgres (vs 222 at phase A start). Open paper positions restored: **0**, because no fill occurred. No duplicate entry is possible without an open position. Unit tests still cover SESSION_ENDED_OPEN reload.

## Predeclared Pons hypotheses (PRELIMINARY, SMALL_SAMPLE)

Unique tokens observed in prospective_signals: 1012. Do not treat 2x counts as expectancy.

| Policy | N obs | N signals | 2x | 5x |
|---|---|---|---|---|
| P0_FIRST_ELIGIBLE_CONTROL | 1012 | 203 | 34 | 5 |
| P1_SOLANA_BUYERS_3_30S | 1012 | 157 | 34 | 5 |
| P2_SOLANA_BUYERS_PLUS_IMBALANCE | 1012 | 161 | 32 | 5 |
| P3_PRICE_WITHOUT_BUYERS_AVOID | 1012 | 203 | 34 | 5 |
| P4_LOW_PARTICIPATION_FILTER | 1012 | 157 | 34 | 5 |

## Quality groups

| Group | Status |
|---|---|
| DISCOVERY | COMPLETE |
| TRADES | COMPLETE |
| STATE | COMPLETE |
| FEATURES | COMPLETE |
| SECURITY | PARTIAL (WARN-only bytecode/policy; not UNKNOWN) |
| CANDIDATES | COMPLETE |
| EXECUTION | MODELLED (orders persisted; fills blocked on missing curve reserves) |
| OUTCOMES | PRELIMINARY |

## Next

Do not live-trade. If a later session obtains exact Pons curve reserves via read-only RPC, re-run paper fills without changing P0–P4 thresholds.
