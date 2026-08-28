"""Outcome labels. Must not be joined onto feature training exports."""

from __future__ import annotations

from .load import load_jsonl
from .validate import FeatureValidationError


def load_outcomes(path: str):
    return load_jsonl(path)


def assert_not_in_features(feature_row: dict) -> None:
    banned = (
        "reached_10x",
        "time_to_10x_ms",
        "max_return_bps",
        "capture_ratio_bps",
        "mfe_quote",
    )
    for k in banned:
        if k in feature_row or k in (feature_row.get("shared") or {}):
            raise FeatureValidationError(f"outcome field {k} leaked into features")
