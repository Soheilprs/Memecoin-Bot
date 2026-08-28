# Solana security

Analyzers: authorities, Token-2022 TLV, metadata, template, sellability.

## Token programs

| Program | Policy |
|---|---|
| SPL Token (`Tokenkeg…`) | supported |
| Token-2022 (`TokenzQd…`) | supported; extensions classified |
| anything else | REJECT (`reject_unknown_token_program`) |

## Token-2022

| Extension | Policy |
|---|---|
| MetadataPointer, TokenMetadata, ImmutableOwner, MemoTransfer, Group* | SAFE |
| TransferFeeConfig, MintCloseAuthority, InterestBearing | WARN |
| Confidential* | UNKNOWN |
| TransferHook, PermanentDelegate, NonTransferable, DefaultAccountState | REJECT |

## Pump.fun exceptions (documented)

Discovery provenance must be the **pinned Pump.fun program**, not the ticker.

- Token-2022 + **MetadataPointer** is expected on `create_v2` → not a reject.
- Mint/freeze authority on the **bonding curve** during the curve is protocol-owned → WARN, not generic mint backdoor.
- Unexpected TransferHook on a “Pump” token → REJECT (template violation).

If mint account bytes are missing at the requested slot: `UNKNOWN_HISTORICAL_STATE`. Current mainnet mint is **not** substituted.

## Sellability

`SellabilityProbe` statuses: SELLABLE / NOT_SELLABLE / UNKNOWN / PROVIDER_LIMITED / NOT_APPLICABLE.

Jupiter quotes are **not** SELLABLE. `simulateTransaction` is not used in free-mode Phase 4 (no user key, no broadcast) → PROVIDER_LIMITED.

Pons `GRADUATION_GAP` is `NOT_APPLICABLE` / protocol transition, **not** a honeypot.
