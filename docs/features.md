# Feature engine (Phase 5)

Point-in-time research features. There is **no opportunity score, ML, simulation, or trading**.

```
Canonical events
      ↓
StateEngine  (TokenState / snapshots)
      ↓
SecurityEngine
      ↓
FeatureEngine  → FeatureVector  (feature_version = 5.0.0)
      ↓
CandidateEngine
```

`ELIGIBLE` means a future strategy **may evaluate** the token. It is not a buy. Phase 6 simulation consumes these vectors; outcomes never write back into them.

## Point-in-time

A vector at `as_of_time` T uses only:

- snapshots with `snapshot_time <= T`
- security assessments with `as_of_time <= T`
- creator / wallet history known before T

It never uses future trades, future graduation time, future price, or later OHLC.

`feature_version` is recorded on every row. Formula changes mint a new version; old vectors are not rewritten.

## Missing ≠ zero

Financial fields are decimal integer strings (`U256`), never `f64`. Optional fields use:

| Encoding | Meaning |
|---|---|
| `{"q":"VALUE","v":…}` | Observed, including `0` |
| `{"q":"UNKNOWN"}` | Not collected / not reconstructed |
| `{"q":"PARTIAL","v":…}` | Lower bound or incomplete source (e.g. PumpSwap reserves) |
| JSON `null` on ratios | Undefined (divide-by-zero). Never Inf/NaN |

Holder concentration, bundle/cluster supply, creator prior rugs/launches, and exit-capacity notionals are **UNKNOWN** until a later enrichment phase. Do not treat them as zero.

## Shared schema (`SharedFeatures`)

Totals: `token_age_ms`, `trade_count_total`, buy/sell counts and unique wallets, quote volumes, net flow, avg/median/max sizes, creator flow, time since last trade/buy/sell.

Windows (trailing, inclusive of T): **5s, 15s, 30s, 60s**. Each has buy/sell counts, unique and new unique wallets, quote volumes, net flow, median/max size, creator volume, count imbalance, buy/sell count ratio (bps or null).

2m/5m/15m remain on the snapshot for later schemas; 5.0.0 does not flatten them.

### Acceleration (no lookahead)

At time T, window length W:

```
unique_buyer_velocity_Ws     = unique_buyers in (T-W, T]
unique_buyer_acceleration_Ws = unique_buyers(T-W, T] − unique_buyers(T-2W, T-W]
```

The previous window is taken from a snapshot with `snapshot_time <= T-W`, never from `(T, T+W]`.

Same formulas for unique sellers, buy/sell quote volume, and net flow (volume deltas as signed decimal strings).

If the prior snapshot is missing, acceleration is UNKNOWN.

Example at T=30s: compare buyers in 15–30s against buyers in 0–15s, **not** 30–45s.

### Imbalance

```
trade_count_imbalance     = buy_count − sell_count
buy_sell_count_ratio_bps  = buy_count * 10000 / sell_count   if sell_count > 0 else null
quote_volume_imbalance    = buy_quote − sell_quote           (signed decimal)
buy_sell_quote_ratio_bps  = buy_quote * 10000 / sell_quote   if sell_quote > 0 else null
```

### Repeat / fresh buyers

When wallet maps are present on the snapshot: `repeat_buyer_count`, `repeat_buyer_ratio_bps`, `mean_buys_per_buyer_milli`, `median_buys_per_buyer`, `new_buyer_ratio_30s_bps`. Otherwise UNKNOWN.

### Liquidity / exit

| Source | `liquidity_quote` |
|---|---|
| Pump bonding curve with real SOL reserves | VALUE |
| PumpSwap constant-product | PARTIAL if a reserve was observed, else UNKNOWN (Phase 3 limitation) |
| Uniswap v4 with `liquidity_raw` | VALUE |
| Missing | UNKNOWN |

`estimated_exit_capacity` and `max_notional_at_{1,2,5}pct` are UNKNOWN until simulation has complete pool state. Do not invent impact.

### Price

`current_price_quote_per_token` is last trade quote/token × 1e18 when both amounts exist. Window returns are bps vs the price on the prior snapshot. Market cap is **not** computed from unverified supply.

`time_to_graduation` is never stored (only known retrospectively). `current_progress_to_graduation_bps` is allowed because it exists at T.

## Protocol namespaces

| Launchpad | `protocol.family` |
|---|---|
| Pump.fun | `solana_pump` — curve progress, virtual/real quote, token reserve |
| Pons V2 | `robinhood_pons` — graduation progress, snipe-tax window (UNKNOWN if unknown) |
| Clanker v4 | `base_clanker` — pool id flag, sqrtPriceX96, liquidity, tick |

Do not compare raw Pump and Pons reserve units.

## Security fields

Copied onto the vector: verdict and risk bands, warning count. They do **not** override the candidate gate. `REJECT` / `UNKNOWN` cannot become `ELIGIBLE`.

## Replay / export

```bash
cargo run -p memecoin-engine -- replay solana tests/fixtures/solana/lifecycle --features
cargo run -p memecoin-engine -- research export-features --out features.jsonl
```

See `docs/candidate-engine.md` and `docs/research-features.md`.
