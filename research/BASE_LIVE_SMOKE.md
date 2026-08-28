# Base live smoke

**Label: LIVE_SMOKE.** Not edge proof. Base PnL is not headline.

## Execution status

**PARTIAL.** Uniswap v4 fill remains `IMPACT_MODEL_PARTIAL_UNISWAP_V4`. Tick liquidity net / tick bitmap are not in `UniswapV4State`. Exact concentrated-liquidity execution is **DEFERRED_SCOPE**. We do **not** approximate v4 as x*y=k.

Shadow orders: `research_valid=false`.

## Session

90-second prospective collect (same process as Robinhood).

| | N |
|---|---|
| tokens | 1 |
| token_discovered | 1 |
| trades | 0 in window |

## Data quality groups

| Group | Status |
|---|---|
| discovery | PARTIAL |
| trades | INCOMPLETE |
| state | PARTIAL |
| features | INCOMPLETE in this window |
| execution | PARTIAL / non-research-valid |
| outcomes | INCOMPLETE |

## Continue

```bash
memecoin-engine research prospective --chains base --duration-secs 1800
```

Do not use Base paper PnL until v4 execution is exact.
