# Cross-chain signal comparison (Pump.fun descriptive vs Pons prospective)

**Verdict: INSUFFICIENT_PONS_SAMPLE**

Do not claim that Pump.fun early-participation signals are useful on Pons from a 30-minute paper window.

## Pump.fun (descriptive, 2 trade shards)

Predeclared H1–H4, not refit.

| Hypothesis | Shard 00016 | Shard 00017 | Pooled / stability |
|---|---|---|---|
| H1 unique_buyers≥3 vs <3 at T+30 p(2x) | 0.216 vs 0.180 | 0.206 vs 0.143 | CONSISTENT_DIRECTION |
| H2 buyers+imbalance vs H3 price-up without buyers p(2x) | 0.335 vs 0.089 | 0.373 vs 0.045 | CONSISTENT_DIRECTION |
| H4 low participation | same as H1 complement | same | thin participation has **lower** 2x (supports the filter; helper label OPPOSITE is p2(low) < p2(high)) |

Usable descriptive labels in processed shards: 21409. EXECUTION_VALID=false.

## Pons (30.7 min prospective, PRELIMINARY)

P0–P4 transferred **without** fitting on Robinhood. Unique tokens 1012. Descriptive 2x among signal tokens is small-sample noise, not edge.

| Policy | signals | 2x |
|---|---|---|
| P0 control | 203 | 34 |
| P1 buyers≥3 | 157 | 34 |
| P2 buyers+imbalance | 161 | 32 |

P1 2x rate 34/157 ≈ 0.22 vs P0 34/203 ≈ 0.17 is **not** a locked result.

## Same-address EVM identity

Unique RH addresses 1048, unique Base 3, both 0. Identity means the same 20-byte address, not the same person.

## Next

Carry H1–H4 **unchanged** into a multi-day locked Pons paper experiment once curve-reserve reads exist for fills. Do not live-trade.
