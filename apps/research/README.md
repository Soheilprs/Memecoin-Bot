# Research helpers (Phase 5)

Load and validate point-in-time `FeatureVector` JSONL. No ML, no trading, no outcomes.

```bash
python3 -m unittest discover -s apps/research/tests
```

```python
from memecoin_research import load_jsonl, validate_vectors

rows = load_jsonl("features.jsonl")
validate_vectors(rows)
```

See `docs/research-features.md`.

## Phase 7.1 corpus

```bash
PYTHONPATH=apps/research python3 -m memecoin_research.dataset acquire --out data/pumpfun/Slinky21_Pumpfun_Memecoin_Corpus --subset
PYTHONPATH=apps/research python3 -m memecoin_research.dataset export-jsonl --dir data/pumpfun/Slinky21_Pumpfun_Memecoin_Corpus --out /tmp/corpus.jsonl --max-rows 500
```

Does not download parquet into Git. Does not invent lamports from float `sol_amount`.

```bash
PYTHONPATH=apps/research python3 -m memecoin_research.moonshot \
  --data-dir data/pumpfun/Slinky21_Pumpfun_Memecoin_Corpus --out-dir research
```

Descriptive moonshot labels only. Not Solana strategy PnL.
