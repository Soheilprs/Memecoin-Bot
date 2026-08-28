# Outcomes (Phase 6)

`OutcomeEngine` may look at the future. `FeatureEngine` must not import `sim`.

Outcome rows live in `token_outcomes`, never in `feature_vectors`. Python `assert_not_in_features` rejects leaked columns.

## Token labels

From a **reference** snapshot (e.g. T+60s) over a horizon:

- `final_return_bps`, `max_return_bps`, `max_drawdown_bps`
- `reached_{2,5,10,20}x` and `time_to_*_ms` (NULL if never)
- Dead tokens and security-rejected tokens are included (no survivorship filter)

Prices used for token labels are spot/curve marks. That is **TOKEN_MAX_RETURN**, distinct from **POSITION_EXECUTABLE_MFE**.

## Position MFE / MAE

After entry, each later snapshot tries an **executable** full exit of remaining tokens (fees+impact, no extra fantasy).

- MFE = highest executable mark quote
- MAE = lowest executable mark quote
- If the market is unsellable, the mark is missing; stop signals still **request** an exit and the engine may `NO_FILL`. End-of-data uses `FORCED_END_OF_DATA`. Dead book → `UNREALIZABLE_POSITION` (not last-print cash-out).

## Capture ratio

When MFE quote > cost:

```
capture_ratio_bps = (realized_quote − cost)+ * 10000 / (mfe_quote − cost)
```

If MFE ≤ cost: NULL.

## End of data

Open positions at the last snapshot: `FORCED_END_OF_DATA` if a mark exists, else `UNREALIZABLE`. Exclude from “strategic exit” stats via `forced_end`.
