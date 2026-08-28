# Execution models (Phase 6)

Versions: `execution/fee/impact/failure 6.0.0`. Scenario priors, **not** live-measured latencies or fitted fees.

## Delay (`FAST` / `BASE` / `SLOW`)

| Chain | FAST | BASE | SLOW |
|---|---|---|---|
| Solana | 500 ms | 2 s | 5 s |
| Base | 2 s | 4 s | 8 s |
| Robinhood | 500 ms | 1 s | 2 s |

Fill uses market state at `T + delay`, not decision-time price. Retries add `retry_delay_ms` and re-quote; they do not reuse the original price.

## Fees (provenance)

| Venue | Scenario | Provenance |
|---|---|---|
| Pump curve | 100 bps protocol | Documented Pump.fun default 1%. Creator bps 0 unless configured. |
| PumpSwap | 25 bps | Scenario combined LP/protocol. Not live-measured. |
| Pons curve | 100 bps | `MEMECOIN_BOT_RESEARCH_V2.md` typical `curveFeeBps`. |
| Pons snipe tax | 9900 bps inside 1 s window | Applied if age < window or `allow_snipe_window`. Confirmation policies do not enter that window; if they did, the tax **destroys** the trade. |
| Clanker / Uni v4 | UNKNOWN | Hook fee not invented. |

Network/priority/tip default `0` quote units unless a scenario sets them.

## Impact

| Venue | Model | Quality |
|---|---|---|
| Pump / Pons bonding curve | virtual reserve CP (`k = virt_sol * virt_token`), fee taken from quote, optional cap at **real** reserves | EXACT if quality COMPLETE; MODELLED if PARTIAL |
| PumpSwap / CP AMM | same `x*y=k` on observed reserves | PARTIAL if Phase 3 marked PumpSwap PARTIAL |
| Uniswap v4 | **not** faked as CP | `IMPACT_MODEL_PARTIAL` → `UNAVAILABLE_MARKET_STATE` for research-valid |
| Unknown / missing reserves | no fill | UNKNOWN ≠ infinite liquidity |

`max_quote_at_{1,2,5}pct_impact` is binary-searched on curve/CP only.

## Slippage

Additional **adverse** bps after impact: BUY receives fewer tokens; SELL receives less quote. Never improves the fill.

## Failure / retry

Seeded SHA-256 (`seed, token, fill_time, attempt`). Default rates 0. Stress: 5%/15% entry. Entry max 1 retry, exit 2, emergency 3.

## Partial fills

If `real_token_reserves` / `real_sol_reserves` cap the trade: `PARTIAL_FILL` with exact amounts.

## Pons graduation gap

`LAUNCH_SWEPT` / `GRADUATION_GAP` → `TEMPORARILY_UNAVAILABLE` (no sell fairy-tale).
