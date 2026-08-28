# Multi-chain research status

Descriptive research and execution research are separate. Do not mix.

## SOLANA

| | |
|---|---|
| Dataset | Slinky21 Pump corpus (decoded tables) |
| Tokens | 798,430 launches (176,130 zero-trade) |
| Features | T+30s/60s/2m/5m counts and unique wallets on processed trades |
| Moonshot labels | Descriptive, price-validated; shard-limited |
| FEATURE_VALID | true |
| DESCRIPTIVE_OUTCOME_VALID | true (labeled subset) |
| EXECUTION_VALID | false |
| Strategy PnL valid? | **no** |

## ROBINHOOD

| | |
|---|---|
| Protocol | Pons V2 (ETH-quoted first) |
| Path | TokenLaunched → state → security → features → candidate → PaperExecutionEngine |
| Execution | Bonding-curve fill math; snipe window blocked; graduation gap unsellable |
| PAPER_LIVE_VALID | software yes; live sample is smoke-duration |

## BASE

| | |
|---|---|
| Protocol | Clanker v4 |
| Path | TokenCreated → state → security → features → candidate → outcomes |
| Execution | Uniswap v4 impact **PARTIAL**; shadow orders `research_valid=false` |
| Exact v4 CL | **DEFERRED_SCOPE** (no tick-liquidity net / tick bitmap in state) |
| Do not approximate as x*y=k | |

## Continue prospective collection

```bash
docker compose up -d postgres
# DATABASE_URL=postgres://memecoin:memecoin@127.0.0.1:5435/memecoin
memecoin-engine research db-check
memecoin-engine research prospective --chains robinhood,base --duration-secs 1800
```

No live capital. No keys. No broadcast.
