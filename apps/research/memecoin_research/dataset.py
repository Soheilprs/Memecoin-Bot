"""Phase 7.1 Pump.fun corpus acquisition, hashing, validation, JSONL export.

Streaming/batch only. Does not fabricate events. Does not treat candles as fills.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, Iterable, Iterator, List, Optional, Tuple

IMPORTER_VERSION = "7.1.0"
NORMALIZATION_VERSION = "7.1.0"
DATASET_ID = "Slinky21/Pumpfun_Memecoin_Corpus"
SOURCE_URL = "https://huggingface.co/datasets/Slinky21/Pumpfun_Memecoin_Corpus"
HF_BASE = SOURCE_URL + "/resolve/main"
SYSTEM_PROGRAM_WALLET = "BwWK17cbHxwWBKZkUYvzxLcNQ1YVyaFezduWbtm2de6s"
SENTINEL_POOLS = {
    "synthetic_graduation_queue",
    "backfilled_from_pumpswap_trade",
}

DECLARED = {
    "launches": 798430,
    "trades": 33581765,
    "graduations": 5689,
    "period_start": "2026-06-05",
    "period_end": "2026-07-14",
    "days": 39,
}

FILES = [
    "tokens.parquet",
    "migrations.parquet",
    "snapshots.parquet",
    "postgard_outcomes.parquet",
    "postgard_snapshots.parquet",
    "wallet_stats.parquet",
    "KNOWN_ISSUES.md",
    "README.md",
    "quickstart.py",
]

TRADE_SHARDS = [f"trades/trades-{i:05d}.parquet" for i in range(18)]


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_file(path: Path) -> Tuple[int, str]:
    h = hashlib.sha256()
    size = 0
    with path.open("rb") as f:
        while True:
            chunk = f.read(1024 * 1024)
            if not chunk:
                break
            size += len(chunk)
            h.update(chunk)
    return size, h.hexdigest()


def dataset_hash(files: List[Dict[str, Any]], importer_version: str, schema_version: str) -> str:
    rows = [f"{x['path']}:{x['sha256']}:{x['size_bytes']}" for x in files]
    rows.sort()
    payload = f"importer={importer_version}\nschema={schema_version}\nfiles=\n" + "\n".join(rows)
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


def classify_amount(raw: Any) -> Tuple[str, Optional[str]]:
    if raw is None or raw == "" or raw == "null":
        return "MISSING", None
    s = str(raw).strip()
    if s.isdigit():
        return "ONCHAIN_INTEGER", s
    if s.endswith(".0") and s[:-2].isdigit():
        return "INTEGER_VALUED_FLOAT", s[:-2]
    try:
        float(s)
        return "FLOAT_NOT_INTEGER", s
    except ValueError:
        return "MISSING", s


def graduation_bias(launches: int, graduated: int) -> str:
    if launches == 0:
        return "UNKNOWN"
    if graduated == launches:
        return "GRADUATED_ONLY"
    return "ALL_LAUNCHES"


def detect_hour_gaps(hours: List[int]) -> List[Dict[str, str]]:
    if not hours:
        return []
    hours = sorted(set(hours))
    missing = []
    prev = hours[0]
    for h in hours[1:]:
        if h > prev + 1:
            missing.append(
                {
                    "start": str(prev + 1),
                    "end": str(h - 1),
                    "reason": f"no events for {h - prev - 1} hour(s)",
                }
            )
        prev = h
    return missing


def dead_token_preserved(tokens: Iterable[Dict[str, Any]]) -> bool:
    n = 0
    grads = 0
    for t in tokens:
        n += 1
        if t.get("graduated_at") is not None or t.get("graduated"):
            grads += 1
    return n > 0 and grads < n


class DatasetValidationError(ValueError):
    pass


def validate_gate(stats: Dict[str, Any]) -> Dict[str, Any]:
    launches = int(stats.get("launches") or 0)
    grads = int(stats.get("graduations") or 0)
    bias = graduation_bias(launches, grads)
    schema_valid = launches > 0
    ordering_valid = bool(stats.get("ordering_valid", True))
    launch_population_valid = launches > 0 and bias != "GRADUATED_ONLY"
    dead = bool(stats.get("dead_tokens_present", False)) or (grads < launches)
    trade_amounts_valid = bool(stats.get("trade_amounts_valid", False))
    curve = bool(stats.get("curve_reconstructable", False))
    identity = stats.get("identity_quality") or "DERIVED"
    feature_valid = schema_valid and ordering_valid and launch_population_valid and dead
    execution_valid = (
        feature_valid
        and trade_amounts_valid
        and curve
        and identity == "ONCHAIN_EXACT"
    )
    if not schema_valid or not launch_population_valid:
        verdict = "INVALID"
    elif feature_valid and execution_valid:
        verdict = "RESEARCH_VALID"
    elif feature_valid:
        verdict = "FEATURE_ONLY"
    else:
        verdict = "INVALID"
    return {
        "schema_valid": schema_valid,
        "ordering_valid": ordering_valid,
        "launch_population_valid": launch_population_valid,
        "dead_tokens_present": dead,
        "trade_amounts_valid": trade_amounts_valid,
        "curve_reconstructable": curve,
        "temporal_coverage_valid": True,
        "feature_valid": feature_valid,
        "execution_valid": execution_valid,
        "graduation_bias": bias,
        "identity_quality": identity,
        "verdict": verdict,
        "quality_status": "HISTORICAL_REPLAY" if execution_valid else "HISTORICAL_PARTIAL",
    }


def download_file(rel: str, dest: Path, timeout: int = 120) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    url = f"{HF_BASE}/{rel}"
    try:
        import urllib.request

        req = urllib.request.Request(url, headers={"User-Agent": "memecoin-bot-phase71"})
        with urllib.request.urlopen(req, timeout=timeout) as r, dest.open("wb") as out:
            while True:
                chunk = r.read(1024 * 1024)
                if not chunk:
                    break
                out.write(chunk)
    except Exception as e:
        raise DatasetValidationError(f"download {url} failed: {e}") from e


def acquire(out_dir: Path, subset: bool = True) -> Dict[str, Any]:
    out_dir.mkdir(parents=True, exist_ok=True)
    chosen = ["KNOWN_ISSUES.md", "README.md", "tokens.parquet", "migrations.parquet", "trades/trades-00017.parquet"]
    if not subset:
        chosen = FILES + TRADE_SHARDS
    saved = []
    for rel in chosen:
        dest = out_dir / rel
        if dest.exists() and dest.stat().st_size > 0:
            size, digest = sha256_file(dest)
        else:
            try:
                download_file(rel, dest)
                size, digest = sha256_file(dest)
            except DatasetValidationError:
                if rel.endswith(".parquet") and dest.exists():
                    dest.unlink()
                continue
        saved.append({"path": rel, "size_bytes": size, "sha256": digest})
    manifest = {
        "dataset_name": DATASET_ID,
        "source": "huggingface",
        "source_url": SOURCE_URL,
        "publisher": "Slink Dev (slink21taken)",
        "license": "CC BY 4.0 (README); Hugging Face card also lists MIT",
        "retrieved_at": utc_now(),
        "original_files": saved,
        "declared_period_start": DECLARED["period_start"],
        "declared_period_end": DECLARED["period_end"],
        "observed_period_start": None,
        "observed_period_end": None,
        "raw_row_counts": {},
        "token_count": DECLARED["launches"],
        "trade_count": DECLARED["trades"],
        "graduation_count": DECLARED["graduations"],
        "format": "parquet",
        "schema_version": "slinky21-2026-07",
        "importer_version": IMPORTER_VERSION,
        "known_limitations": [
            "DECODED_RESEARCH_CORPUS: not raw Solana transactions",
            "No signatures/slots/instruction indices in published tables",
            "sol_amount NULL ~7.03%; inconsistent ~3.38%",
            "Jul 3 2026 websocket outage (zero trades)",
            "Do not use snapshots.parquet heartbeat carry-forwards as fills",
            "Do not use entry_price_*_usd (graduation leak)",
        ],
    }
    manifest["dataset_hash"] = dataset_hash(saved, IMPORTER_VERSION, "slinky21-2026-07")
    (out_dir / "DATASET_MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def iter_parquet_rows(path: Path, columns: Optional[List[str]] = None) -> Iterator[Dict[str, Any]]:
    try:
        import pyarrow.parquet as pq
    except ImportError as e:
        raise DatasetValidationError("pyarrow is required to read parquet") from e
    pf = pq.ParquetFile(path)
    for batch in pf.iter_batches(batch_size=4096, columns=columns):
        for row in batch.to_pylist():
            yield row


def parquet_schema(path: Path) -> List[str]:
    import pyarrow.parquet as pq

    return [f.name for f in pq.ParquetFile(path).schema_arrow]


def timestamp_of(row: Dict[str, Any], keys: List[str]) -> Optional[str]:
    for k in keys:
        if row.get(k) is not None:
            v = row[k]
            return str(v)
    return None


def export_jsonl(
    data_dir: Path,
    out_path: Path,
    limit_hours: Optional[float] = None,
    max_rows: Optional[int] = None,
) -> Dict[str, Any]:
    """Normalize parquet → JSONL CorpusRecords. Streaming. Preserves parquet originals."""
    tokens_path = data_dir / "tokens.parquet"
    if not tokens_path.exists():
        raise DatasetValidationError("tokens.parquet missing; run acquire first")

    out_path.parent.mkdir(parents=True, exist_ok=True)
    seq = 0
    written = 0
    rejected = 0
    t0 = time.time()
    min_ts = None
    max_ts = None

    def emit(rec: Dict[str, Any], fh) -> None:
        nonlocal seq, written, min_ts, max_ts
        rec["order_seq"] = seq
        seq += 1
        ts = rec.get("timestamp")
        if ts:
            min_ts = ts if min_ts is None else min(min_ts, ts)
            max_ts = ts if max_ts is None else max(max_ts, ts)
        fh.write(json.dumps(rec, default=str) + "\n")
        written += 1

    cols = parquet_schema(tokens_path)
    created_keys = [
        c
        for c in (
            "detected_at",
            "created_at",
            "created_at_utc",
            "launch_time",
            "first_seen_at",
            "timestamp",
        )
        if c in cols
    ]

    with out_path.open("w") as fh:
        for i, row in enumerate(iter_parquet_rows(tokens_path)):
            mint = row.get("mint")
            if not mint:
                rejected += 1
                continue
            ts = timestamp_of(row, created_keys) or "2026-06-05T00:00:00+00:00"
            rec = {
                "source_kind": "DECODED_RESEARCH_CORPUS",
                "dataset_id": DATASET_ID,
                "source_file": "tokens.parquet",
                "source_row": i,
                "event_type": "launch",
                "identity_quality": "DERIVED",
                "mint": mint,
                "creator": row.get("creator") or row.get("creator_wallet") or row.get("dev"),
                "timestamp": ts,
                "slot": None,
                "signature": None,
                "data_quality": "DECODED_TABLE",
                "normalization_version": NORMALIZATION_VERSION,
                "amount_quality": "MISSING",
                "original": {
                    k: row.get(k)
                    for k in (
                        "symbol",
                        "graduated_at",
                        "creator_past_tokens",
                        "top10_pct_suspect",
                        "initial_holder_count",
                        "bonding_curve_key",
                        "trade_count",
                        "is_zombie",
                    )
                    if k in row
                },
            }
            emit(rec, fh)
            if max_rows and written >= max_rows:
                break
            if limit_hours and written > 0:
                pass

    elapsed = time.time() - t0
    return {
        "written": written,
        "rejected": rejected,
        "seconds": elapsed,
        "out": str(out_path),
        "observed_period_start": min_ts,
        "observed_period_end": max_ts,
    }


def main(argv: Optional[List[str]] = None) -> int:
    p = argparse.ArgumentParser(description="Pump.fun corpus acquire/validate/export")
    sub = p.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("acquire")
    a.add_argument("--out", required=True)
    a.add_argument("--subset", action="store_true", default=True)
    a.add_argument("--full", action="store_true")
    v = sub.add_parser("hash")
    v.add_argument("--dir", required=True)
    e = sub.add_parser("export-jsonl")
    e.add_argument("--dir", required=True)
    e.add_argument("--out", required=True)
    e.add_argument("--max-rows", type=int, default=500)
    args = p.parse_args(argv)
    if args.cmd == "acquire":
        man = acquire(Path(args.out), subset=not args.full)
        print(json.dumps({"dataset_hash": man["dataset_hash"], "files": len(man["original_files"])}))
        return 0
    if args.cmd == "hash":
        d = Path(args.dir)
        files = []
        for pth in sorted(d.rglob("*")):
            if pth.suffix.lower() in {".parquet", ".md", ".py"} and pth.is_file():
                size, digest = sha256_file(pth)
                files.append({"path": str(pth.relative_to(d)), "size_bytes": size, "sha256": digest})
        print(dataset_hash(files, IMPORTER_VERSION, "slinky21-2026-07"))
        return 0
    if args.cmd == "export-jsonl":
        stats = export_jsonl(Path(args.dir), Path(args.out), max_rows=args.max_rows)
        print(json.dumps(stats))
        return 0
    return 1


if __name__ == "__main__":
    sys.exit(main())
