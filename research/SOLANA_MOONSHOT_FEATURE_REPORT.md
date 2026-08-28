# Solana moonshot feature report (descriptive)

**SOLANA FEATURE RESEARCH VERDICT: EXPANDED** (2/18 trade shards). Not FULL_CORPUS_COMPLETE.

Not execution PnL. `EXECUTION_VALID = false`. No strategy expectancy is claimed.

## Dataset

Slinky21/Pumpfun_Memecoin_Corpus. Tokens parquet fully counted. Trades streamed: `trades-00016.parquet` + `trades-00017.parquet`. Checkpoint: `research/SOLANA_SHARD_CHECKPOINT.json`. A third shard download failed on disk (5.8 GiB free). Remaining shards were not co-resident.

| Population | N |
|---|---|
| All launches | 798,430 |
| Zero-trade (`trade_count=0`) | 176,130 |
| Active in processed shards | 21,871 |
| Usable descriptive labels | 21,409 |
| Invalid price labels | 462 |

Dead tokens are counted. Analysis is not restricted to migrations.

## Capabilities

| | |
|---|---|
| FEATURE_VALID | true (counts / unique wallets from actual trades) |
| DESCRIPTIVE_OUTCOME_VALID | true for labeled shard tokens after price checks |
| EXECUTION_VALID | **false** |

Volumes, holder concentration, bundle supply, mint authority: **UNKNOWN** (not fabricated).

## Price validation

Invalid if missing/zero/negative/non-finite. Invalid rows cannot produce 2x/5x/10x labels. Heartbeat/identical rows are not counted as new trades.

## Cohorts (usable labels in processed shards 00016+00017)

| Cohort | N |
|---|---|
| <2X | 17,356 |
| 2X+ | 2,873 |
| 5X+ | 670 |
| 10X+ | 255 |
| 20X+ | 255 |
| DEAD / NO TRADE (full corpus) | 176,130 |

Baseline among usable labels: P(2x)=0.189, P(5x)=0.055, P(10x)=0.024.

Two independent time slices. Treat rates as **descriptive on processed shards**, not corpus-wide moonshot base rates.

## Early features (before the large move)

Question: what is visible before the move that separates later moonshots from dead/low-activity tokens?

**Not** “10x tokens had rising price.” Price-up with **no** buyer growth had **P(10x)=0** at T+30s (n=60).

### Unique buyers ≥ 3 vs < 3 (pooled processed shards)

| Horizon | n (≥3) | P(2x) | P(10x) | P(2x) if <3 buyers |
|---|---|---|---|---|
| T+30s | 6974 | 0.215 | 0.013 | 0.177 |
| T+60s | 8945 | 0.264 | 0.027 | 0.136 |
| T+2m | 10139 | 0.290 | 0.038 | 0.099 |
| T+5m | 10830 | 0.298 | 0.044 | 0.078 |

H1 direction is **consistent** on both shards: unique_buyers≥3 has higher p(2x) than the complement. Lift vs this 2-shard baseline is modest at T+30 and larger at later horizons. Thin participation is associated with fewer subsequent 2x.

### Pairwise

Buy-count growth **and** buy/sell imbalance > 0 vs price-up without buyer growth:

| Horizon | n (buyers+flow) | P(10x) | n (price only) | P(10x) |
|---|---|---|---|---|
| T+30s | 6112 | 0.026 | 424 | 0.005 |
| T+60s | 6554 | 0.039 | 400 | 0.000 |
| T+2m | 6727 | 0.051 | 231 | 0.000 |
| T+5m | 6665 | 0.061 | 159 | 0.000 |

Volume features remain UNKNOWN.

## HYPOTHESES_FOR_EXECUTION_VALIDATION

Do not claim they make money.

1. Unique buyers ≥ 3 by T+30s / T+60s (broad early participation).
2. Buyer growth **plus** positive buy/sell count imbalance (not price-only).
3. Avoid entries whose only early signal is price-up with no new buyers.
4. Zero-trade / unique-buyers < 3 as a **dead-token filter**, not a long.
5. Re-test (1)–(3) on Robinhood Pons paper and on execution-valid Solana later.

## Artifacts

- `research/SOLANA_MOONSHOT_COHORTS.json`
- `research/SOLANA_MOONSHOT_FEATURES.jsonl` (processed-shard tokens; gitignored if large)
