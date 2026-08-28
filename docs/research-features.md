# Research feature export

JSONL of `FeatureVector` rows for notebooks. No parquet dependency in Phase 5. No outcomes, no labels, no PnL.

```bash
cargo run -p memecoin-engine -- research export-features --chain solana --out features.jsonl --limit 10000
```

Requires `DATABASE_URL`. Replay into Postgres with `--persist --features` first.

## Sample columns (JSONL object)

```
chain, token_address, launchpad
snapshot_id, security_assessment_id
as_of_block, as_of_slot, as_of_time, token_age_ms
feature_version          # "5.0.0"
data_quality, flow_quality, liquidity_quality, holder_quality, creator_quality
fingerprint
shared.buy_count_total, shared.unique_buyers_total, shared.net_quote_flow_total
shared.win15.unique_buyers, shared.unique_buyer_acceleration_15s
shared.holder_count      # {"q":"UNKNOWN"} until Phase 5.5+
shared.creator_prior_rugs
protocol.family          # solana_pump | robinhood_pons | base_clanker | none
```

Join candidates separately from `candidate_state_transitions` on `(chain, token_address, as_of_time)` plus `policy_id`.

Do not join future `token_trades` onto a vector. Point-in-time is `as_of_time`.

## Python helper

`apps/research` loads JSONL and checks:

- `feature_version` present
- no `opportunity_score`
- `holder_count` UNKNOWN is not coerced to 0
- per-token `as_of_time` is non-decreasing
- per-token `buy_count_total` is non-decreasing (no lookahead mix-up)

```bash
python3 -m unittest discover -s apps/research/tests
```

Phase 6 will add outcomes (entry delay, fees, impact, MFE/MAE). Do not compute them here.
