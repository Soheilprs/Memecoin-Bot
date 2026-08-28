"""Fail closed on lookahead / missing-as-zero mistakes. No model training."""

from __future__ import annotations

from typing import Any, Dict, List, Tuple


class FeatureValidationError(ValueError):
    pass


def _opt_is_unknown(field: Any) -> bool:
    return isinstance(field, dict) and field.get("q") == "UNKNOWN"


def _opt_value(field: Any) -> Any:
    if isinstance(field, dict) and field.get("q") == "VALUE":
        return field.get("v")
    return None


def validate_vectors(rows: List[Dict[str, Any]]) -> List[str]:
    """Return warnings. Raises FeatureValidationError on hard failures."""
    warnings: List[str] = []
    if not rows:
        return ["empty export"]

    last_time: Dict[Tuple[str, str], str] = {}
    last_buys: Dict[Tuple[str, str], int] = {}

    for i, row in enumerate(rows):
        if "opportunity_score" in row or "OPPORTUNITY_SCORE" in row:
            raise FeatureValidationError(f"row {i}: opportunity_score is forbidden in Phase 5")
        if not row.get("feature_version"):
            raise FeatureValidationError(f"row {i}: missing feature_version")
        if "as_of_time" not in row:
            raise FeatureValidationError(f"row {i}: missing as_of_time")
        shared = row.get("shared") or {}
        holder = shared.get("holder_count")
        if holder == 0:
            raise FeatureValidationError(
                f"row {i}: holder_count is integer 0; missing must be UNKNOWN"
            )
        rugs = shared.get("creator_prior_rugs")
        if rugs == 0:
            raise FeatureValidationError(
                f"row {i}: creator_prior_rugs integer 0; unknown history must be UNKNOWN"
            )
        if holder is not None and not _opt_is_unknown(holder) and _opt_value(holder) is None:
            if isinstance(holder, dict) and holder.get("q") not in ("VALUE", "PARTIAL", "UNKNOWN"):
                warnings.append(f"row {i}: unexpected holder_count encoding {holder!r}")

        key = (str(row.get("chain")), str(row.get("token_address")))
        t = str(row["as_of_time"])
        if key in last_time and t < last_time[key]:
            raise FeatureValidationError(
                f"row {i}: as_of_time went backwards for {key} ({t} < {last_time[key]})"
            )
        last_time[key] = t
        buys = int(shared.get("buy_count_total") or 0)
        if key in last_buys and buys < last_buys[key]:
            raise FeatureValidationError(
                f"row {i}: buy_count_total decreased for {key}; possible lookahead mix"
            )
        last_buys[key] = buys

    return warnings
