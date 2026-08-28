# Phase 7.4 qualification (Pons execution-state + prospective paper)

Not an edge test. No headline PnL. No live transactions. No keys.

`PIPELINE_SMOKE_POLICY` fills have `alpha_research_valid = false` and
`execution_model_valid = true` when `curve_state_quality` is `EXACT_BLOCK_READ`.
P0–P4 definitions are unchanged from Phase 7.3 and are **not** fitted here.

## Session (primary 60-minute run)

| | |
|---|---|
| Start | 2026-08-27 20:21:38 UTC |
| Restart | 2026-08-27 20:51:48 UTC (phase A → B) |
| End | 2026-08-27 21:22:39 UTC |
| Wall | 3661 s (~61 min) including planned restart |
| RH blocks | 47724173–47742006 then 47742498–47760334 |
| Base blocks | 50536380–50537106 then …–50538197 |
| Gaps | 0 (`LIVE_COMPLETE`) |

Earlier the same evening (20:07–20:21 UTC) two short aborted runs produced the
first live fills and left **71 OPEN** paper positions. Those were restored at
20:21:45 (`recovered=71`) and later closed by `X1_TIME_2M`. Phase B restored
**164** open positions. That is the live restart-recovery proof.

## Counts (Robinhood unless noted; window 20:07:50–21:22:39 UTC)

| Item | N |
|---|---|
| TokenLaunched / TOKEN_CREATED | 662 |
| CurveBuy | 8264 |
| CurveSell | 7091 |
| SnipeTaxCharged | 523 |
| LaunchSwept | 9 |
| PoolGraduated | 9 |
| raw events | 17127 |
| feature vectors | 91400 |
| security assessments | 662 WARN, 0 PASS, 0 REJECT, 0 UNKNOWN |
| candidate ELIGIBLE (distinct tokens) | 180 |
| candidate transitions ELIGIBLE | 284 |
| P0 enter observations | 9755 |
| P1 enter observations | 6046 |
| P2 enter observations | 364 |
| P3 enter observations | 9679 |
| P4 enter observations | 6046 |
| smoke paper orders | 692 |
| fills | 682 |
| fill rate | 98.6% |
| unavailable | 10 (`INVALID_CURVE_STATE`, zero `getReserves`) |
| positions opened (smoke) | 682 |
| positions closed | 348 |
| still OPEN at process exit | 334 |
| `pons_curve_states` | 1202, all `EXACT_BLOCK_READ` |
| RPC rate limits observed | 0 |
| multicall batches | 0 (sequential getters; correctness independent of batching) |
| Base TokenCreated | 20 |
| Base shadow orders | 2194 (`IMPACT_MODEL_PARTIAL_UNISWAP_V4`) |

Smoke fills are plumbing. Do **not** mix them into P0–P4 research statistics.

## Curve state

Verified getters from `PonsV2BondingCurve.sol` (`v2-bondingcurve-getters-1`):

- `getReserves()` → CP virtual quote + token
- `realQuoteReserve()` → physically held tradeable quote
- `sellableTokens()` → buy cap
- `graduationThreshold()`, `graduated()`, `readyToGraduate()`
- `feeBps()`, `creatorTaxBps()`

Historical `eth_call` on the configured free Robinhood RPC: **supported** at
latest, head−10, and head−100 (tested at head 47714778).

Event reconstruction: **not used**. CurveBuy/Sell logs do not carry virtual
reserves. No `RECONSTRUCTED_VALIDATED` mark.

## Execution quality (one real paper fill)

Token `0x1c874de4…464eb706` (truncated):

| Field | Value |
|---|---|
| status | FILLED |
| curve_state_quality | EXACT_BLOCK_READ |
| execution_quality | MODELLED_HIGH_CONFIDENCE |
| data_quality | LIVE_COMPLETE |
| alpha_research_valid | false |
| execution_model_valid | true |
| quote | 1000000000 |
| protocol_fee | 40000000 (on-chain fee+tax overlay) |
| snipe_tax | 0 (outside 1s window) |
| price_impact_bps | 416 |
| delay | wall wait then sim delay 0 |

## Restart recovery

Token `0xa3540be2…d91cf1f`:

1. Smoke BUY filled 20:08:18 UTC, `EXACT_BLOCK_READ`
2. Process stopped (aborted run)
3. Restored at 20:21:45 among 71 OPEN positions (remaining tokens + quote cost intact)
4. No duplicate entry
5. `X1_TIME_2M` TIME_STOP close 20:24:11 UTC, remaining tokens 0

## Outcome maturity

| Maturity | N (session writes) |
|---|---|
| PENDING | 4319 (age &lt; 1h) |
| MATURE | 15 (1h horizon complete in-session) |
| CENSORED_SESSION_END | 0 observed on this run |

Tick-task shutdown was not joined during the 60-minute process, so session-end
censor/SESSION_ENDED_OPEN rows were not flushed. The join is now in `collect.rs`.
PENDING vs MATURE classification by age is correct.

## Security WARN

Every Pons assessment is WARN. Top (only) warning check: `TEMPLATE_BYTECODE` —
runtime hash is not pinned (`runtime_bytecode_hash = None`). Factory match is
not treated as PASS. Not silently upgraded.

## P0–P4

Unchanged: `P0_FIRST_ELIGIBLE_CONTROL`, `P1_SOLANA_BUYERS_3_30S`,
`P2_SOLANA_BUYERS_PLUS_IMBALANCE`, `P3_PRICE_WITHOUT_BUYERS_AVOID`,
`P4_LOW_PARTICIPATION_FILTER`. No threshold edits after observing this session.
No edge claims.

## Base / Solana

Base remains live shadow only. Solana remains **EXPANDED 2/18** shards
(`trades-00016`, `trades-00017`). Failed `trades-00015` download is
**DISK_LIMITATION**, not a Solana ingest failure and not corpus completion.

## Next

Spec: `research/PONS_PROSPECTIVE_EXP001_SPEC.md` (not started).

If multiday readiness is READY, next is Phase 7.5 — locked multi-day Pons paper.
Do not live-trade.
