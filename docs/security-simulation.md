# Security simulation

## What runs offline

A **local isolated plan**, not a mainnet Anvil fork and not a broadcast:

```
BUY → TRANSFER to wallet B → SELL 50% → SELL remainder → SELL from B
```

Deterministic test EOAs. No user private keys. No real capital.

Results are valid **as of the isolated fork block recorded on the assessment**. They do not mean the token can never become a honeypot later (owner may change tax/blacklist).

Timeout → `UNKNOWN` / `SIMULATION_FAILED`, never PASS.

## Honeypot labels

| Result | Meaning |
|---|---|
| NOT_HONEYPOT | sells succeeded on this fork |
| HONEYPOT | sell reverts or ≥90% extracted |
| CONDITIONAL | first sell or first wallet works; later/other wallet fails |
| UNKNOWN / SIMULATION_FAILED | no model / timeout |

High sell tax vs `max_sell_tax_bps` is a separate hard reject.

## Limitations

Live Base/RH Uniswap v4 fork against Anvil + archive RPC is **not** enabled in Phase 4 (no paid node). Tests use explicit `TokenSimModel` (synthetic market mechanics) plus static bytecode analysis of real fixtures.

External APIs (GoPlus, Honeypot.is, RugCheck) are optional evidence interfaces only; they are not called and are never the sole basis for PASS.
