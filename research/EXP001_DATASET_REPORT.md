# EXP001 dataset report

**DATASET VERDICT: FEATURE_ONLY**

Not `RESEARCH_VALID`. Not `INVALID`. Not `NOT_FOUND`.

## Source

Exact V1 citation, not a substitute:

- Hugging Face: [Slinky21/Pumpfun_Memecoin_Corpus](https://huggingface.co/datasets/Slinky21/Pumpfun_Memecoin_Corpus)
- Publisher: Slink Dev (slink21taken)
- License: CC BY 4.0 in README; the Hugging Face card also lists MIT. Recorded as a discrepancy; both noted. No paid provider.
- Provenance: websocket + on-chain RPC polling, then **decoded tables**. `source_kind = DECODED_RESEARCH_CORPUS`.
- This is **not** raw Solana transaction bytes.

## Manifest

See `data/pumpfun/Slinky21_Pumpfun_Memecoin_Corpus/DATASET_MANIFEST.json`.

| Field | Declared |
|---|---|
| Period | 2026-06-05 → 2026-07-14 (39 days) |
| Format | Parquet shards |
| tokens.parquet | 798,430 rows (~214 MB) |
| trades/trades-00000..00017.parquet | 33,581,765 rows (~5.5 GB) |
| snapshots.parquet | 26,934,864 rows (~571 MB) |
| migrations.parquet | 5,701 rows |
| postgard_outcomes.parquet | 5,669 rows |
| Total archive | 6.7 GB |

Local subset checksums (this machine, 2026-08-27):

| File | Size | SHA-256 |
|---|---|---|
| tokens.parquet | 214,405,469 | `c005d86d424013e5c78701161b025f3d8c3d472afb61466e0ad6fd5afe9e8ea6` (matches HF LFS oid) |
| migrations.parquet | 480,591 | `ef5d5141fd94acbcd121bed50e39e525cf7777d25338fc9852a67fd8a085105d` |
| trades/trades-00017.parquet | 27,991,939 | `3e48808f2ee97c7238af48446ef1ee2028b93fed58f11715dce6ef5a15fbe580` |

Subset `dataset_hash` (files + importer 7.1.0 + schema `slinky21-2026-07`): `5ae3cbca393b1556305982a0833adb31b6d8ab83b9ab1709d530341b4cd21b78`

Huge files are gitignored. Full 6.7 GB archive was not downloaded (disk ~8.5 GB free).

## Schema (from KNOWN_ISSUES + quickstart, not invented)

**tokens (observed schema):** mint, detected_at, creator, bonding_curve_key, graduated_at, trade_count, is_zombie, v_sol/v_tokens as float64, concentration `*_corrected`, leaky `entry_price_*_usd`.

**trades (observed schema on shard 00017):** id, mint, **tx_signature**, event_time, seconds_since_launch, is_buy, sol_amount, token_amount, user_wallet, v_tokens_bonding_curve, v_sol_bonding_curve, market_cap_sol, price_sol, curve_pct_depleted, source. No slot, no instruction index.

**migrations:** mint, migrated_at, seconds_to_graduation, pool_address (1,380 `synthetic_graduation_queue`, 69 `backfilled_from_pumpswap_trade`).

Missing vs execution bar: slot, transaction_index, instruction_index, inner_instruction_index, integer on-chain lamports (SOL/token amounts are floats). Signatures exist on trades but do not make identity `ONCHAIN_EXACT` without ix indices.

## Coverage

Observed from local `tokens.parquet` (798,430 rows, 798,430 unique mints — matches V1):

| | Count |
|---|---|
| Launches | **798,430** observed (declared 798,430) |
| Graduated (`graduated_at` not null) | **5,689** (declared 5,689) |
| Migrations table | 5,701 rows / 5,701 unique mints |
| Zero-trade launches (`trade_count=0`) | 176,130 |
| `is_zombie` | 170,003 |
| Launches with ≥1 trade | 622,300 |
| Launches with ≥10 trades | 347,380 |
| Trades (declared all shards) | 33,581,765 |
| Trades shard 00017 observed | 158,405 rows, 151,504 unique signatures |
| Dead / never-graduated | 792,741 |
| Duplicate mints | 0 |

Graduation-bias check: **ALL_LAUNCHES** (not survivor-only). Dead tokens remain.

Observed launch `detected_at`: 2026-06-05 09:12:26Z → 2026-07-14 15:01:42Z.

Missing launch days (37 populated days in a 39-day declared window):

- **2026-06-18** (unexplained in KNOWN_ISSUES)
- **2026-07-03** (documented websocket outage)
- **2026-07-12** (unexplained in KNOWN_ISSUES)

These are recorded, not hidden.

## Validation

| Gate | Result |
|---|---|
| schema_valid | true (mint, side, timestamps, launches, trades, grads) |
| ordering_valid | deterministic timestamp + event type + mint + source_row (no slot/ix) |
| launch_population_valid | true |
| dead_tokens_present | true |
| trade_amounts_valid | **false** — ~7.03% NULL sol_amount, ~3.38% inconsistent, values are floats |
| curve_reconstructable | **false** — no integer protocol reserves suitable for Phase 6 fill math |
| temporal_coverage_valid | true with documented Jul 3 gap |
| FEATURE_VALID | **true** |
| EXECUTION_VALID | **false** |

Why FEATURE_VALID: every launch in the window is present, including zero-trade and failed coins; trades have time, side, wallet; ordering can be made deterministic; we can rebuild count/timing features without lookahead if we ignore leak columns.

Why not EXECUTION_VALID: Phase 6 fill math is integer U256 on bonding-curve reserves. This corpus stores decoded floats, omits signature/slot/ix, has NULL and inconsistent SOL amounts, and its 15s snapshots are ~90–95% heartbeat carry-forwards. Using `price_sol` / OHLC as a fill would be the candle fantasy Phase 6 forbade. We do **not** convert `0.01` SOL into invented lamports.

## Identity

`identity_quality = DERIVED`. Corpus event id from dataset_id + file + row + type + mint + order_seq. Not `ONCHAIN_EXACT`.

## Spot-check vs chain

Trades include `tx_signature`. Deterministic sample of **20** signatures from `trades-00017.parquet` against free `api.mainnet-beta.solana.com` `getTransaction`:

| Check | Result |
|---|---|
| signature found on chain | **20 / 20** |
| corpus mint in transaction account keys | **20 / 20** |
| slots (on-chain, not in parquet) | e.g. 432860798 … 432875327 |

This is a validation sample only. We did not reconstruct the corpus via RPC. Slots exist on chain but are **not** in the published tables, so replay identity remains `DERIVED`.

Phase 1–3 fixture mint `wv7hXQuSg8bfTheL183WJhheQVKrFBidsjvq9YFpump` is **not** in `tokens.parquet` (N/A).

## Import path

Streaming: parquet row groups → JSONL CorpusRecord → `PumpCorpusSource` → `DecoderRegistry` (`pumpfun_corpus` 7.1.0) → canonical events → StateEngine.

Do not load the 6.7 GB archive into RAM. Do not commit parquet to Git.

Local disk at implementation time had ~8.5 GB free; a full 6.7 GB download plus JSONL expansion is a **DATASET_BLOCKER** for a complete local copy on this machine. Subset acquire (tokens + migrations + last trade shard) is the intended first import.

## Acceptance

`quality_status = HISTORICAL_PARTIAL`, `complete = false`.

Not `HISTORICAL_REPLAY`.
