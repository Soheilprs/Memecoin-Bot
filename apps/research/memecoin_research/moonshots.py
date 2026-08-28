"""Missed-winner tables. Descriptive only."""

from __future__ import annotations

from typing import Any, Dict, List, Optional


def filter_moonshots(rows: List[Dict[str, Any]], min_max_return_bps: int = 40000) -> List[Dict[str, Any]]:
    return [r for r in rows if int(r.get("max_return_bps") or 0) >= min_max_return_bps]


def recall_bps(entered_hits: int, observable_hits: int) -> Optional[int]:
    if observable_hits == 0:
        return None
    return (entered_hits * 10000) // observable_hits


def precision_bps(entered_hits: int, entered: int) -> Optional[int]:
    if entered == 0:
        return None
    return (entered_hits * 10000) // entered
