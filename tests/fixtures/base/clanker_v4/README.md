# Clanker v4 fixtures

See `docs/fixtures.md`.

- Factory: `0xE85A59c628F7d27878ACeB4bf3b35733630083a9`
- PoolManager: `0x498581fF718922c3f8e6A244956aF099B2652b2b`

| File | Event | Block | Tx |
|---|---|---|---|
| `token_created.json` | TokenCreated | 50514417 | `0xee43dac9…` |
| `token_created_for_swap.json` | TokenCreated (companion) | 50506751 | `0x1ff9fc40…` |
| `swap.json` | Uniswap v4 Swap | 50506751 | `0x1ff9fc40…` |

Swap matching: `Swap` topic1 equals `TokenCreated.poolId`.
