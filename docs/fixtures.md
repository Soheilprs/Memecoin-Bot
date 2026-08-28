# Fixtures

All fixtures are **real on-chain events**. Tests load them from disk and do not call RPCs.

## Pump.fun CreateV2

| Field | Value |
|---|---|
| Path | `tests/fixtures/solana/pumpfun/create_v2.json` |
| Instruction | `create_v2` |
| Signature | `4YXdATSTkXijNXCNWc8t6Wikh1JbHjqCvfJGDymThRcPyGsayyKrRr9bLiBhbj92rAgGQvAmeiZYN7ZHzmpHRpNG` |
| Slot | `442063149` |

## Pump.fun buy

| Field | Value |
|---|---|
| Path | `tests/fixtures/solana/pumpfun/buy.json` |
| Signature | `2rwCH19aXEtGDfSrY3tHqqYizoEWJ5d3m77G8NnDvWZTYjmwW6h6wyHzAuUtYFVyaMzPGNskV6X2cQZmgrUDzYFs` |
| Slot | `442063169` |
| Event | Anchor `TradeEvent` (`is_buy=true`) |

## Pump.fun sell

| Field | Value |
|---|---|
| Path | `tests/fixtures/solana/pumpfun/sell.json` |
| Signature | `pnJWC4yrcmD2BBRtzRq92dsVvi5jvDVdDigXs9MbFznWxdLDBSSpaXyJBo6Eex2YGKB28zBC8HsvydHYntRDSLu` |
| Slot | `442063173` |
| Event | Anchor `TradeEvent` (`is_buy=false`) |

Buy/sell were captured from signatures of mint `5aHRzARp74osQhG6SQ3rPDzKxSCEPt7duGhwZBt1pump` (the CreateV2 fixture token).

## Golden Pump.fun lifecycle (graduated)

Real token `wv7hXQuSg8bfTheL183WJhheQVKrFBidsjvq9YFpump`. Replay directory `tests/fixtures/solana/lifecycle/`.

| File | Event | Slot | Signature (prefix) |
|---|---|---|---|
| `create.json` | CreateV2 | 442088680 | `2DhPvNLrQ8V2KJMY…` |
| `buy.json` | Buy (same tx as create) | 442088680 | `2DhPvNLrQ8V2KJMY…` |
| `migrate.json` | MigrateV2 | 442088680 | `51Pr48Kw5gZjQRg8…` |
| `create_pool.json` | PumpSwap CreatePool (inner of migrate) | 442088680 | `51Pr48Kw5gZjQRg8…` |
| `pamm_sell.json` | PumpSwap Sell | 442089973 | `3jF6uAEfKuTuQvd4…` |

- Bonding curve: `7KH4HscCwK2Bi1y4Ldhsaf9shagXiihAWZxWi4cR3atf`
- PumpSwap pool: `5XKoFuwq8fwMLtLyTEDeg1SXTny4YsAeP8RuWTRPZU81`

`memecoin-engine replay solana tests/fixtures/solana/lifecycle` runs these through the production decoder. Do not replace them with synthetic events.

## Pons V2 TokenLaunched

| Field | Value |
|---|---|
| Path | `tests/fixtures/robinhood/pons_v2/token_launched.json` |
| Tx | `0xbe5330a3c03a2da76e63a38cfacbd1b17bc78df9d67bf0ca74e63fac04aeba58` |
| Block | `47295513` |
| topic0 | `0x8d4aad4953d0ca700d468f3753aa14432d1b35b43ec6409f051fb6aa43a89607` |

## Pons V2 CurveBuy

| Field | Value |
|---|---|
| Path | `tests/fixtures/robinhood/pons_v2/curve_buy.json` |
| Tx | `0x971851a1268d9eb30df93fab7d5d63d14e63a6b2e73f894de565c9057b4d3c98` |
| Block | `47325947` |
| logIndex | `50` |
| topic0 | `0xec36bf571f136799e8dc0b0b8bea4b04d8bd3d43de838aab0d5fc21d4cbfc455` |

## Pons V2 CurveSell

| Field | Value |
|---|---|
| Path | `tests/fixtures/robinhood/pons_v2/curve_sell.json` |
| Tx | `0xc216700e273b197c026db3845bd7b05ecd1aec8bf8c05eb146e19582d171cb78` |
| Block | `47325950` |
| logIndex | `23` |
| topic0 | `0x8113d738abdcb6b38357e9d53a54a7157861a09031b453651f0fe7fe151f59df` |

## Pons V2 LaunchSwept

| Field | Value |
|---|---|
| Path | `tests/fixtures/robinhood/pons_v2/launch_swept.json` |
| Tx | `0x1054de60e3d0cc9b4b8c728246e220a603042cf9185fa3740ddfd23880bd3dd8` |
| Block | `47302618` |
| logIndex | `51` |
| topic0 | `0xcdb72f157fd3666758a6ce201387ffb52038c7562e4fff352828da1096c4b6b4` |

## Pons V2 PoolGraduated

| Field | Value |
|---|---|
| Path | `tests/fixtures/robinhood/pons_v2/pool_graduated.json` |
| Tx | `0x9ebf0d93fc852de1957fd7598785b6b6c69237acb991516add39b360a49a0eec` |
| Block | `47302692` |
| logIndex | `66` |
| topic0 | `0x0a44ef75df69c534f43cd6c1aa3ef8983065fe5fe79ef9e79f6494e6f258c259` |

LaunchSwept at 47302618 and PoolGraduated at 47302692 are **74 blocks apart** (~7s). They must not be collapsed.

## Clanker v4 TokenCreated

| Field | Value |
|---|---|
| Path | `tests/fixtures/base/clanker_v4/token_created.json` |
| Tx | `0xee43dac94f85495553135818f0544bd21ffa8ab22b264d4f28c426cdf9464a55` |
| Block | `50514417` |

## Clanker v4 Swap (Uniswap v4)

| Field | Value |
|---|---|
| Path | `tests/fixtures/base/clanker_v4/swap.json` |
| Companion create | `token_created_for_swap.json` |
| Tx | `0x1ff9fc40fcaeeffa3c52dea8df3d99f8c4102b3dcbaddbefbed83dc478a8b277` |
| Block | `50506751` |
| PoolManager | `0x498581fF718922c3f8e6A244956aF099B2652b2b` |
| Matching | `Swap` topic1 == `TokenCreated.poolId` |

The swap and TokenCreated are in the **same transaction** (swap logIndex 159, create logIndex 167).

## Adding another fixture

1. Capture a real tx/log from a public RPC (development only).
2. Store raw fields plus `provenance`.
3. Pin the ABI/IDL used to decode it.
4. Add an offline `cargo test`.
5. Do not invent addresses or event data.
