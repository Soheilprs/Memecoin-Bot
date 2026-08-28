"""Load FeatureVector JSONL produced by `memecoin-engine research export-features`."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Iterable, List, Union


def load_jsonl(path: Union[str, Path]) -> List[dict[str, Any]]:
    rows: List[dict[str, Any]] = []
    with Path(path).open() as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            rows.append(json.loads(line))
    return rows


def iter_jsonl(path: Union[str, Path]) -> Iterable[dict[str, Any]]:
    with Path(path).open() as f:
        for line in f:
            line = line.strip()
            if line:
                yield json.loads(line)
