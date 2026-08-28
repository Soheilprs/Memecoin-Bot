# Strategy research (Phase 7)

No live trading. No opportunity score. Runtime strategies **must not** import `TokenOutcome`, MFE, or time-to-X.

```
FeatureVector + Candidate + Security
        → EntryStrategy (S0–S6)
        → Phase 6 ExecutionEngine (unchanged fill math)
        → PositionManager (X1–X9)
        → OutcomeEngine (evaluation only)
```

## Splits

Chronological 60 / 20 / 20 by time. TEST is unused until `StrategyExperiment::lock()` hashes the config. Editing the locked policy yields `CONFIG_DRIFT`.

## Families

| ID | Rule (TRAIN thresholds only) |
|---|---|
| S0 | Phase 6 baselines (first / delayed / random eligible) |
| S1 | buyer accel ≥ train Q50 and velocity ≥ Q50 |
| S2 | S1 + non-negative net flow + buy/sell imbalance ≥ 0 |
| S3 | unique buyers + cap on repeat-buyer ratio + new-buyer ratio |
| S4 | another family plus `creator_has_sold = false` (not a Security REJECT) |
| S5 | curve progress band + unique buyers |
| S6 | hybrid of accel, flow, participation, no creator sell, curve band |

Exits add `X7_FLOW_DECAY`, `X8_CREATOR_SELL`, `X9_DYNAMIC_RUNNER` (partials + flow/creator/time cap).

## EXP001

Headline research requires `FEATURE_VALID` **and** `EXECUTION_VALID` (`quality_status = HISTORICAL_REPLAY` or `LIVE_COMPLETE`). The Slinky21 decoded corpus is `FEATURE_ONLY` until integer curve state, slot, and transaction identity exist. Fixtures are too small. If execution-invalid: **EXP001_BLOCKED_DATASET**. TEST runs once after lock; a second run is refused.

See `research/EXP001_REPORT.md`.
