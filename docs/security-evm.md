# EVM security (Base + Robinhood)

Pipeline: template → bytecode fingerprint → proxy → privileges → mechanics → optional local simulation.

## Templates (required fast paths)

Clanker v4 (Base) and Pons V2 (Robinhood).

Matching uses **factory provenance + optional runtime hash + expected hook/pool manager**. Factory address alone is **not** PASS.

If provenance says Clanker/Pons but bytecode hash is pinned and differs: `TEMPLATE_MISMATCH` → full analyzer.

Pons legitimate snipe tax / protocol fees are not treated as a hidden 99% honeypot tax.

## Fingerprints

- exact runtime keccak256
- Solidity metadata-stripped hash
- PUSH4 selector set (collisions are evidence, not rejects)

## Proxy

EIP-1967 implementation/admin/beacon slots, EIP-1822/UUPS selectors, EIP-1167 clones.

EOA-controlled upgradeability on a meme is a hard reject unless an explicit template exception.

DELEGATECALL is flagged with context; proxies are not auto-rejected for DELEGATECALL alone.

## Privileges

Selectors for owner, mint, pause, blacklist, tax setters, maxTx, upgrade, AccessControl.

`owner() == 0` is **not** proof of renouncement if mint/tax/blacklist/upgrade/roles remain (fake renounce).

Verified explorer source is **never** treated as safety. Runtime bytecode is authoritative.
