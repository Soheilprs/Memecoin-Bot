# Moonshot / missed-winner analysis

For tokens with `max_return >= 5x` or `10x`, record whether each baseline policy entered and how much it captured.

## Miss reasons

| Reason | Meaning |
|---|---|
| `SECURITY_REJECT` | Hard security gate. Do not treat a later pump as proof the gate was wrong. |
| `SECURITY_UNKNOWN` | Fail closed; not research-valid entry |
| `NEVER_ELIGIBLE` | Candidate never reached ELIGIBLE |
| `ENTRY_FAILED` | Order attempted, seeded/state failure |
| `LIQUIDITY_TOO_LOW` | No executable market |
| `EXITED_TOO_EARLY` | Entered; capture_ratio on a 10x token below 25% |
| `ENTERED_CAPTURED` | Entered and held a material fraction of MFE |

## Baseline matrix (not optimized)

Entries: `E1_FIRST_ELIGIBLE`, `E2/E3/E4` delay 30/60/120s, `E5_RANDOM_ELIGIBLE_CONTROL` (seeded 50%).

Exits: time 2m/5m/15m, fixed TP/SL (+100%/−50%), trailing 30% from peak, partial runner (+40/80/200% sell 20% each, 40% runner + trail).

The runner is **not** flattened just because it is in profit.

Phase 7 will compare these using train/test splits. Phase 6 does not grid-search.
