# EXP001 — first locked strategy experiment

**SOFTWARE:** Phase 7.1 importer, validation gate, lock/TEST refusal.  
**DATASET VERDICT: FEATURE_ONLY**  
**RESEARCH VERDICT: EXP001_BLOCKED_DATASET**

## Hypothesis

Early unique-buyer acceleration and confirmation delay (30–120s) identify a subset with positive post-cost expectancy and non-trivial 5x/10x capture versus first-eligible and random-eligible baselines.

## Dataset

Exact V1 source: Hugging Face `Slinky21/Pumpfun_Memecoin_Corpus` (2026-06-05 → 2026-07-14, CC BY 4.0).

`source_kind = DECODED_RESEARCH_CORPUS`. Identity `DERIVED`. FeatureVector **5.0.0** unchanged.

| Field | Value |
|---|---|
| FEATURE_VALID | true |
| EXECUTION_VALID | false |
| quality_status | HISTORICAL_PARTIAL |
| complete | false |
| dataset_hash | `5ae3cbca…4cd21b78` (subset: tokens+migrations+trades-00017 + importer 7.1.0) |

See `research/EXP001_DATASET_REPORT.md`.

## Split (predeclared; unused for TEST)

Chronological 60% TRAIN / 20% VALIDATION / 20% TEST by discovery time. A token assigned to TRAIN keeps later lifecycle in TRAIN. TEST untouched until lock.

Cannot publish exact split timestamps/counts until a full local import exists. Do not invent them.

## Configurations budget (predeclared)

- Hypotheses: 8 (H7.1–H7.8)
- Baseline entries × exits: 5 × 6 = 30 (Phase 6)
- Families S1–S6 × 3 TRAIN quantiles (Q40/Q50/Q60) × 3 exits (X2, X6, X9) = 54
- **Variants evaluated (max predeclared): 84**
- VALIDATION: at most 5
- TEST: 1 primary + 1 alternate, hashed lock

No 5,000-combination search. No ML.

## TRAIN feature findings (H7.1–H7.8)

Not computed on FeatureVector 5.0.0 from this corpus. Running them would require replaying trades through StateEngine with honest point-in-time snapshots. Volume features would be UNKNOWN because amounts are not on-chain integers. We do not substitute corpus `price_sol` into FeatureEngine formulas.

## Strategies evaluated

0 on execution-valid fills. Software still encodes S0–S6 / X1–X9.

## VALIDATION

No finalists. Gate failed before lock.

## Locked policy

**Not locked.** Locking a strategy against FEATURE_ONLY data and then reporting TEST PnL would be p-hacking theatre.

Template (unchanged):

```
entry: S1_BUYER_GROWTH (thresholds from TRAIN Q50 only)
exit: X9_DYNAMIC_RUNNER
size: native quote units (not USD)
execution: BASE delay, fee model 6.0.0, 0 bps extra slip
```

## Out-of-sample TEST

**Not run. TEST RUN COUNT = 0.**

Software refuses:

- `TEST_ALREADY_RUN` on a second attempt
- `DATASET_HASH_MISMATCH` if the corpus changes
- `DATASET_NOT_RESEARCH_VALID` / `EXP001_BLOCKED_DATASET` when `EXECUTION_VALID=false`

No fills, expectancy, median, trimmed mean, win rate, profit factor, or max drawdown are claimed.

## Baseline / right-tail / PnL concentration / missed 10x / exits / catastrophic / stress / size / CI

N/A. Would be invalid on candle/float fills.

## Historical security policy

UNKNOWN ≠ PASS. Mint-at-slot authority is `UNKNOWN_HISTORICAL_STATE` / `REQUIRES_ARCHIVE_STATE`. Launchpad provenance from the corpus label is the only historically labeled pump.fun evidence. Current chain state is not substituted. This alone blocks research-valid strategy simulation.

## Test audit

| | |
|---|---|
| dataset hash | `5ae3cbca393b1556305982a0833adb31b6d8ab83b9ab1709d530341b4cd21b78` (local subset) |
| split hash | not computed (TEST not entered) |
| config hash | not locked |
| git commit | working tree Phase 7.1 |
| feature_version | 5.0.0 |
| importer_version | 7.1.0 |
| seed | 1 |
| TEST RUN COUNT | **0** (allowed max 1) |

## Verdict

`EXP001_BLOCKED_DATASET`

Still missing for strategy expectancy:

1. Integer on-chain token and SOL amounts (not float `sol_amount`)
2. Virtual/real curve reserves as **on-chain integers** (corpus stores float64; many `v_sol` values are integer-valued but that is not `ONCHAIN_INTEGER`)
3. Slot and instruction/inner-instruction ordering (trade **signatures exist** and 20/20 matched chain; slots are not in the parquet)
4. Historical mint account state at T; UNKNOWN ≠ PASS and blocks research-valid simulation
5. A local complete copy of all trade shards (this machine: tokens + migrations + trades-00017 only; ~8.5 GB free vs 6.7 GB full archive)

Do not begin Phase 8.
