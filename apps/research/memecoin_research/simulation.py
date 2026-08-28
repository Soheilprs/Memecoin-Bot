"""Load simulation JSONL. No ML."""

from __future__ import annotations

from .load import load_jsonl


def load_simulations(path: str):
    return load_jsonl(path)
