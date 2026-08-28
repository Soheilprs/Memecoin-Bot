# Security engine (Phase 4)

Hard gate between discovery/state and any future strategy.

```
DISCOVERED / TokenState
        ↓
SecurityWorkQueue (bounded; never silent drop)
        ↓
SecurityEngine.assess  (same code for live and replay)
        ↓
PASS | WARN | REJECT | UNKNOWN
```

**SECURITY ≠ OPPORTUNITY.** High volume does not pass a freeze authority, honeypot, or 99% sell tax.

## Verdicts

| Verdict | Meaning |
|---|---|
| PASS | Required checks completed without hard rejects or blocking unknowns |
| WARN | Issues that are not automatic rejects (mutable metadata, unpinned template hash, protocol gap) |
| REJECT | Hard reject (mint backdoor, freeze, transfer hook, honeypot, EOA upgrade admin, …) |
| UNKNOWN | Missing data, timeout, provider limit, historical state unavailable |

**UNKNOWN is never PASS.** Timeouts and RPC failures stay UNKNOWN.

Assessments are **append-only** with `analyzer_version` + `policy_version`. Old rows are not rewritten.

`RPC_DEV_INCOMPLETE` on the source is copied onto the assessment and not upgraded.

## Policy (`SecurityPolicy::phase4_defaults`)

Provisional safety limits, not a tuned 0–100 score:

- `max_buy_tax_bps` / `max_sell_tax_bps` = 1000 (10%)
- reject active freeze, unknown token program, EOA upgrade admin, arbitrary mint, transfer hook, permanent delegate, non-transferable
- `require_sellability` = false (UNKNOWN sell probe does not by itself PASS or REJECT)

See `docs/security-solana.md`, `docs/security-evm.md`, `docs/security-simulation.md`.

## CLI (read-only)

```bash
memecoin-engine security fixture tests/fixtures/solana/pumpfun/create_v2.json
memecoin-engine security token --chain base --token 0x... --launchpad clanker_v4
```

No broadcast. No user keys.

Phase 5: `REJECT` → candidate `SECURITY_REJECTED`; `UNKNOWN` never `ELIGIBLE`. See `docs/candidate-engine.md`.
