# Graduated Pump.fun token lifecycle

Real token `wv7hXQuSg8bfTheL183WJhheQVKrFBidsjvq9YFpump`.

Why last-50 mint signatures miss migrate: after graduation the mint's recent signatures are PumpSwap AMM trades. Migrate lives on the **bonding curve** (and is often the same transaction as PumpSwap `CreatePool`). Search the curve/pool, not the last 50 mint txs.

| File | Event | Slot | Signature |
|---|---|---|---|
| `create.json` | CreateV2 | 442088680 | `2DhPvNLrQ8V2KJMY…` |
| `buy.json` | Buy (same tx as create) | 442088680 | `2DhPvNLrQ8V2KJMY…` |
| `migrate.json` | **MigrateV2** | 442088680 | `51Pr48Kw5gZjQRg8…` |
| `create_pool.json` | PumpSwap CreatePool (CPI inner 22 of migrate) | 442088680 | `51Pr48Kw5gZjQRg8…` |
| `pamm_sell.json` | PumpSwap Sell | 442089973 | `3jF6uAEfKuTuQvd4…` |

No pre-migration sell exists for this token (curve completed on the create+buy tx). Current on-chain migrate instruction name is `MigrateV2` (`migrate_v2` discriminator).

- Bonding curve: `7KH4HscCwK2Bi1y4Ldhsaf9shagXiihAWZxWi4cR3atf`
- PumpSwap pool: `5XKoFuwq8fwMLtLyTEDeg1SXTny4YsAeP8RuWTRPZU81`
- IDL: `0.1.0`
