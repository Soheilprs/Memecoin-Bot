# Historical replay

Offline Solana replay uses the **same** `DecoderRegistry` as live collection. There is no second parser.

```
fixtures / jsonl / decoded corpus JSONL
    → HistoricalSource::next_event
    → DiscoveryPipeline (DecoderRegistry::production)
    → TokenDiscovered / TradeObserved / LifecycleObserved
    → StateEngine  (--snapshots)
    → token_state_snapshots
```

This is the seed of a future `HistoricalExecutionEngine`. No strategy logic lives here.

## Commands

```bash
memecoin-engine replay solana tests/fixtures/solana/lifecycle
memecoin-engine replay solana tests/fixtures/solana/lifecycle --persist
```

`--persist` writes canonical rows to `DATABASE_URL`. Without it, replay uses an in-memory store and prints a report.

`SOLANA_MODE=historical` is not a live collector. `collect solana --mode historical` errors and points at `replay`.

## Sources

```rust
trait HistoricalSource {
    async fn next_event(&mut self) -> Result<Option<RawEvent>>;
}
```

| Implementation | Status |
|---|---|
| `FixtureSource` | Implemented. Loads a directory of real JSON fixtures, dedupes by event id, orders by slot / signature / instruction. |
| `JsonlSource` | Streams one JSON object per line. Never loads the whole file into RAM. Accepts RawEvent or CorpusRecord. |
| `PumpCorpusSource` | Implemented. Streaming JSONL of `DECODED_RESEARCH_CORPUS` rows. Identity is `DERIVED` unless signature+slot+ix are present. |
| Parquet files | Read by the Python importer (`memecoin_research.dataset`) in batches and written to JSONL. Original parquet is preserved. |

## Golden lifecycle fixture

Token `wv7hXQuSg8bfTheL183WJhheQVKrFBidsjvq9YFpump`

```
CreateV2
    ↓
Buy
    ↓
MigrateV2
    ↓
PumpSwap CreatePool
    ↓
PumpSwap Sell/Swap
```

Directory: `tests/fixtures/solana/lifecycle/`. Curve `7KH4HscCwK2Bi1y4Ldhsaf9shagXiihAWZxWi4cR3atf`. Pool `5XKoFuwq8fwMLtLyTEDeg1SXTny4YsAeP8RuWTRPZU81`.

Replay is deterministic: two runs produce the same canonical fingerprint. A second pass on the same store is all duplicates.

## Pump.fun corpus (Phase 7.1)

Exact V1 source: Hugging Face [`Slinky21/Pumpfun_Memecoin_Corpus`](https://huggingface.co/datasets/Slinky21/Pumpfun_Memecoin_Corpus) (CC BY 4.0; card also lists MIT).

This is a **decoded research table**, not raw chain bytes.

```
parquet (preserved)
    → Python streaming importer
    → JSONL CorpusRecord
    → PumpCorpusSource
    → RawEventKind::DecodedCorpus
    → PumpCorpusDecoder
    → canonical events
    → StateEngine
```

`source_kind = DECODED_RESEARCH_CORPUS`. Do not claim `ONCHAIN_EXACT` without signature/slot/instruction index.

### Commands

```bash
python3 -m memecoin_research.dataset acquire --out data/pumpfun/Slinky21_Pumpfun_Memecoin_Corpus --subset
memecoin-engine corpus validate path/to/corpus.jsonl --manifest data/pumpfun/Slinky21_Pumpfun_Memecoin_Corpus/DATASET_MANIFEST.json
memecoin-engine corpus replay path/to/corpus.jsonl --snapshots --features
```

Large parquet/jsonl files are gitignored. Commit the manifest and importer only.

### Validation gate

`validate_historical_dataset()` must run before EXP001.

`HISTORICAL_REPLAY` / `complete=true` is allowed only when the gate sets `execution_valid=true` and the launch population is not graduated-only.

Otherwise: `HISTORICAL_PARTIAL`. Strategy PnL is blocked.

OHLC/candles are never executable fills. Float `sol_amount` is never converted into invented lamports.

Research/simulation must still call `validate_dataset_quality()` so an accidental `rpc_dev` session cannot silently feed a backtest.

## Quality

Real on-chain fixture sessions are `HISTORICAL_REPLAY` and `complete=true` for the covered window. That is **not** a claim that live Solana is complete, and it is **not** automatic for the decoded Hugging Face corpus.
