"""Solana descriptive moonshot features. Not execution PnL. Not FeatureVector mutation."""

from __future__ import annotations

import hashlib
import json
import math
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Tuple

SYSTEM_PROGRAM_WALLET = "BwWK17cbHxwWBKZkUYvzxLcNQ1YVyaFezduWbtm2de6s"
HORIZONS_MS = {
    "5s": 5_000,
    "15s": 15_000,
    "30s": 30_000,
    "60s": 60_000,
    "2m": 120_000,
    "5m": 300_000,
    "15m": 900_000,
}
REPORT_HORIZONS = ["30s", "60s", "2m", "5m"]


def is_heartbeat(prev: Optional[Dict[str, Any]], cur: Dict[str, Any]) -> bool:
    if not prev:
        return False
    return (
        prev.get("mint") == cur.get("mint")
        and prev.get("event_time") == cur.get("event_time")
        and prev.get("price_sol") == cur.get("price_sol")
        and prev.get("sol_amount") == cur.get("sol_amount")
        and prev.get("user_wallet") == cur.get("user_wallet")
        and prev.get("is_buy") == cur.get("is_buy")
    )


def valid_price(p: Any) -> bool:
    try:
        x = float(p)
    except (TypeError, ValueError):
        return False
    return math.isfinite(x) and x > 0.0


def price_quality(prices: List[float], missing: int, n: int) -> str:
    if n == 0:
        return "INVALID"
    miss = missing / max(n, 1)
    if any(p <= 0 or not math.isfinite(p) for p in prices):
        return "INVALID"
    if miss > 0.5:
        return "INVALID"
    if miss > 0.1:
        return "DESCRIPTIVE_PARTIAL"
    return "DESCRIPTIVE_HIGH"


def return_bps(ref: float, px: float) -> Optional[int]:
    if ref <= 0 or px <= 0 or not math.isfinite(ref) or not math.isfinite(px):
        return None
    return int(round((px / ref - 1.0) * 10_000))


@dataclass
class MintState:
    mint: str
    launch_ms: int
    creator: Optional[str] = None
    trade_count_declared: int = 0
    buys: int = 0
    sells: int = 0
    wallets_buy: set = field(default_factory=set)
    wallets_sell: set = field(default_factory=set)
    buy_count_by_wallet: Dict[str, int] = field(default_factory=dict)
    first_price: Optional[float] = None
    prices: List[Tuple[int, float]] = field(default_factory=list)
    missing_price: int = 0
    n_trade_rows: int = 0
    snapshots: Dict[str, Dict[str, Any]] = field(default_factory=dict)
    frozen: set = field(default_factory=set)

    def observe_trade(self, age_ms: int, is_buy: bool, wallet: Optional[str], price: Optional[float]) -> None:
        self.n_trade_rows += 1
        if wallet and wallet != SYSTEM_PROGRAM_WALLET:
            if is_buy:
                self.wallets_buy.add(wallet)
                self.buy_count_by_wallet[wallet] = self.buy_count_by_wallet.get(wallet, 0) + 1
                self.buys += 1
            else:
                self.wallets_sell.add(wallet)
                self.sells += 1
        elif is_buy:
            self.buys += 1
        else:
            self.sells += 1
        if valid_price(price):
            px = float(price)
            if self.first_price is None:
                self.first_price = px
            self.prices.append((age_ms, px))
        else:
            self.missing_price += 1
        for name, h in HORIZONS_MS.items():
            if name in self.frozen:
                continue
            if age_ms <= h:
                self.snapshots[name] = self.feature_at(age_ms)
            elif name not in self.frozen:
                if name not in self.snapshots:
                    self.snapshots[name] = self.feature_at(h)
                self.frozen.add(name)

    def feature_at(self, age_ms: int) -> Dict[str, Any]:
        buys = self.buys
        sells = self.sells
        ub = len(self.wallets_buy)
        us = len(self.wallets_sell)
        repeats = sum(1 for c in self.buy_count_by_wallet.values() if c > 1)
        px = None
        px0 = self.first_price
        ret = None
        for a, p in self.prices:
            if a <= age_ms:
                px = p
        if px is not None and px0:
            ret = return_bps(px0, px)
        return {
            "token_age_ms": age_ms,
            "trade_count": buys + sells,
            "buy_count": buys,
            "sell_count": sells,
            "unique_buyers": ub,
            "unique_sellers": us,
            "buyer_seller_imbalance": buys - sells,
            "repeat_buyers": repeats,
            "creator_traded": False,
            "price_change_bps": ret,
            "buy_volume": None,
            "sell_volume": None,
            "holder_count": None,
            "bundle_supply": None,
        }

    def finalize_horizons(self) -> None:
        for name, h in HORIZONS_MS.items():
            if name not in self.snapshots:
                self.snapshots[name] = self.feature_at(min(h, 0 if self.n_trade_rows == 0 else h))
                if self.n_trade_rows == 0:
                    self.snapshots[name]["token_age_ms"] = h

    def descriptive_outcome(self) -> Dict[str, Any]:
        q = price_quality(
            [p for _, p in self.prices],
            self.missing_price,
            self.n_trade_rows,
        )
        if self.n_trade_rows == 0:
            q = "DESCRIPTIVE_HIGH"
        ref = self.first_price
        series = self.prices
        reached = {"2x": False, "5x": False, "10x": False, "20x": False}
        tto = {"2x": None, "5x": None, "10x": None, "20x": None}
        mx = {"5m": None, "15m": None, "30m": None, "1h": None}
        max_bps = None
        if q != "INVALID" and ref:
            for age, px in series:
                bps = return_bps(ref, px)
                if bps is None:
                    continue
                max_bps = bps if max_bps is None else max(max_bps, bps)
                for label, need in (("2x", 10_000), ("5x", 40_000), ("10x", 90_000), ("20x", 190_000)):
                    if bps >= need and not reached[label]:
                        reached[label] = True
                        tto[label] = age
                if age <= 300_000:
                    mx["5m"] = px if mx["5m"] is None else max(mx["5m"], px)
                if age <= 900_000:
                    mx["15m"] = px if mx["15m"] is None else max(mx["15m"], px)
                if age <= 1_800_000:
                    mx["30m"] = px if mx["30m"] is None else max(mx["30m"], px)
                if age <= 3_600_000:
                    mx["1h"] = px if mx["1h"] is None else max(mx["1h"], px)
            if q == "INVALID":
                reached = {k: False for k in reached}
        if q == "INVALID":
            reached = {k: False for k in reached}
            tto = {k: None for k in tto}
        cohort = "DEAD / NO TRADE"
        if self.n_trade_rows == 0 and self.trade_count_declared == 0:
            cohort = "DEAD / NO TRADE"
        elif q == "INVALID":
            cohort = "INVALID"
        elif max_bps is not None and max_bps >= 190_000:
            cohort = "20X+"
        elif max_bps is not None and max_bps >= 90_000:
            cohort = "10X+"
        elif max_bps is not None and max_bps >= 40_000:
            cohort = "5X+"
        elif max_bps is not None and max_bps >= 10_000:
            cohort = "2X+"
        elif max_bps is not None and max_bps <= -5_000:
            cohort = "LOSS >50%"
        elif self.n_trade_rows == 0:
            cohort = "DEAD / NO TRADE"
        else:
            cohort = "<2X"
        return {
            "token": self.mint,
            "quality": q,
            "cohort": cohort,
            "max_return_bps": max_bps,
            "reached_2x": reached["2x"] if q != "INVALID" else False,
            "reached_5x": reached["5x"] if q != "INVALID" else False,
            "reached_10x": reached["10x"] if q != "INVALID" else False,
            "reached_20x": reached["20x"] if q != "INVALID" else False,
            "time_to_2x_ms": tto["2x"] if q != "INVALID" else None,
            "time_to_5x_ms": tto["5x"] if q != "INVALID" else None,
            "time_to_10x_ms": tto["10x"] if q != "INVALID" else None,
            "time_to_20x_ms": tto["20x"] if q != "INVALID" else None,
            "execution_valid": False,
        }


def process_events(tokens: Iterable[Dict[str, Any]], trades: Iterable[Dict[str, Any]]) -> List[MintState]:
    states: Dict[str, MintState] = {}
    for t in tokens:
        mint = t["mint"]
        states[mint] = MintState(
            mint=mint,
            launch_ms=int(t.get("launch_ms") or 0),
            creator=t.get("creator"),
            trade_count_declared=int(t.get("trade_count") or 0),
        )
    prev = None
    ordered = sorted(
        trades,
        key=lambda r: (r.get("event_time") or 0, r.get("id") or 0, r.get("mint") or ""),
    )
    for row in ordered:
        if is_heartbeat(prev, row):
            prev = row
            continue
        prev = row
        mint = row.get("mint")
        if mint not in states:
            states[mint] = MintState(mint=mint, launch_ms=int(row.get("launch_ms") or 0))
        st = states[mint]
        age = int(row.get("age_ms") or 0)
        st.observe_trade(age, bool(row.get("is_buy")), row.get("user_wallet"), row.get("price_sol"))
    for st in states.values():
        st.finalize_horizons()
    return list(states.values())


def cohort_stats(states: List[MintState]) -> Dict[str, Any]:
    usable = []
    for st in states:
        o = st.descriptive_outcome()
        if o["quality"] == "INVALID":
            continue
        usable.append((st, o))
    n = len(usable)
    def rate(pred):
        if n == 0:
            return None
        return round(sum(1 for _, o in usable if pred(o)) / n, 6)

    pop = {
        "all_launches": len(states),
        "zero_trade": sum(1 for s in states if s.n_trade_rows == 0 and s.trade_count_declared == 0),
        "active": sum(1 for s in states if s.n_trade_rows > 0),
        "invalid_labels": sum(1 for s in states if s.descriptive_outcome()["quality"] == "INVALID"),
        "usable_labels": n,
    }
    cohorts = defaultdict(int)
    for _, o in usable:
        cohorts[o["cohort"]] += 1
    p2 = rate(lambda o: o["reached_2x"])
    p5 = rate(lambda o: o["reached_5x"])
    p10 = rate(lambda o: o["reached_10x"])
    lifts = {}
    for hz in REPORT_HORIZONS:
        high_buyers = []
        low_buyers = []
        for st, o in usable:
            feat = st.snapshots.get(hz) or {}
            ub = feat.get("unique_buyers") or 0
            if ub >= 3:
                high_buyers.append(o)
            else:
                low_buyers.append(o)
        def pr(xs, key):
            if not xs:
                return None
            return round(sum(1 for o in xs if o[key]) / len(xs), 6)
        lifts[hz] = {
            "unique_buyers_ge_3": {
                "n": len(high_buyers),
                "p2": pr(high_buyers, "reached_2x"),
                "p5": pr(high_buyers, "reached_5x"),
                "p10": pr(high_buyers, "reached_10x"),
                "baseline_p2": p2,
                "baseline_p5": p5,
                "baseline_p10": p10,
            },
            "unique_buyers_lt_3": {"n": len(low_buyers), "p2": pr(low_buyers, "reached_2x")},
        }
    pairwise = {}
    for hz in REPORT_HORIZONS:
        both = []
        price_only = []
        for st, o in usable:
            f = st.snapshots.get(hz) or {}
            accel_proxy = (f.get("buy_count") or 0) >= 2
            px = f.get("price_change_bps")
            price_up = isinstance(px, int) and px > 0
            if accel_proxy and (f.get("buyer_seller_imbalance") or 0) > 0:
                both.append(o)
            if price_up and not accel_proxy:
                price_only.append(o)
        pairwise[hz] = {
            "buyers_plus_net_flow_n": len(both),
            "buyers_plus_net_flow_p10": (round(sum(1 for o in both if o["reached_10x"]) / len(both), 6) if both else None),
            "price_up_no_buyers_n": len(price_only),
            "price_up_no_buyers_p10": (round(sum(1 for o in price_only if o["reached_10x"]) / len(price_only), 6) if price_only else None),
        }
    return {
        "population": pop,
        "cohorts": dict(cohorts),
        "baseline": {"p2": p2, "p5": p5, "p10": p10},
        "lifts": lifts,
        "pairwise": pairwise,
        "execution_pnl_claimed": False,
        "capabilities": {
            "FEATURE_VALID": True,
            "DESCRIPTIVE_OUTCOME_VALID": n > 0,
            "EXECUTION_VALID": False,
        },
    }


def hypothesis_h1_h4(states: List[MintState], hz: str = "30s") -> Dict[str, Any]:
    """Predeclared. unique_buyers>=3; buyers+imbalance; price-up without buyers; low participation."""
    rows = []
    for st in states:
        o = st.descriptive_outcome()
        if o["quality"] == "INVALID":
            continue
        f = st.snapshots.get(hz) or {}
        ub = int(f.get("unique_buyers") or 0)
        imb = int(f.get("buyer_seller_imbalance") or 0)
        px = f.get("price_change_bps")
        price_up = isinstance(px, int) and px > 0
        buyer_growth = ub >= 2 or (f.get("buy_count") or 0) >= 2
        rows.append((o, ub, imb, price_up, buyer_growth))
    n = len(rows)
    def pr(pred):
        xs = [o for o, ub, imb, pu, bg in rows if pred(ub, imb, pu, bg)]
        if not xs:
            return {"n": 0, "p2": None, "p10": None}
        return {
            "n": len(xs),
            "p2": round(sum(1 for o in xs if o["reached_2x"]) / len(xs), 6),
            "p10": round(sum(1 for o in xs if o["reached_10x"]) / len(xs), 6),
        }
    return {
        "n": n,
        "H1_buyers_ge_3": pr(lambda ub, imb, pu, bg: ub >= 3),
        "H1_complement": pr(lambda ub, imb, pu, bg: ub < 3),
        "H2_buyers_plus_imbalance": pr(lambda ub, imb, pu, bg: bg and imb > 0),
        "H3_price_without_buyers": pr(lambda ub, imb, pu, bg: pu and not bg),
        "H4_low_participation": pr(lambda ub, imb, pu, bg: ub < 3),
        "predeclared": True,
    }


def deterministic_hash(payload: Any) -> str:
    raw = json.dumps(payload, sort_keys=True, default=str).encode()
    return hashlib.sha256(raw).hexdigest()


def process_shards_with_resume(
    tokens: List[Dict[str, Any]],
    shards: List[Tuple[str, List[Dict[str, Any]]]],
    checkpoint: Optional[Path] = None,
) -> List[MintState]:
    done: List[str] = []
    if checkpoint and checkpoint.exists():
        done = json.loads(checkpoint.read_text()).get("done", [])
    remaining = [(n, rows) for n, rows in shards if n not in done]
    trades: List[Dict[str, Any]] = []
    finished = list(done)
    for name, rows in remaining:
        trades.extend(rows)
        finished.append(name)
        if checkpoint:
            checkpoint.write_text(json.dumps({"done": finished}))
    return process_events(tokens, trades)


def shard_checksum(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def direction_consistency(per_shard: Dict[str, Any]) -> Dict[str, Any]:
    """H1: unique_buyers>=3 should have higher p2 than complement, if both n>0."""
    out: Dict[str, Any] = {}
    keys = [
        ("H1", "H1_buyers_ge_3", "H1_complement"),
        ("H2", "H2_buyers_plus_imbalance", "H3_price_without_buyers"),
        ("H4", "H4_low_participation", "H1_buyers_ge_3"),
    ]
    for label, pos, neg in keys:
        dirs = []
        for name, row in per_shard.items():
            h = row.get("hypotheses") or {}
            a = h.get(pos) or {}
            b = h.get(neg) or {}
            pa, pb = a.get("p2"), b.get("p2")
            if pa is None or pb is None:
                dirs.append({"shard": name, "direction": "INSUFFICIENT"})
            elif pa > pb:
                dirs.append({"shard": name, "direction": "POS_GT_NEG", "p2_pos": pa, "p2_neg": pb})
            elif pa < pb:
                dirs.append({"shard": name, "direction": "POS_LT_NEG", "p2_pos": pa, "p2_neg": pb})
            else:
                dirs.append({"shard": name, "direction": "TIE", "p2_pos": pa, "p2_neg": pb})
        usable = [d for d in dirs if d["direction"] in ("POS_GT_NEG", "POS_LT_NEG", "TIE")]
        pos_gt = sum(1 for d in usable if d["direction"] == "POS_GT_NEG")
        pos_lt = sum(1 for d in usable if d["direction"] == "POS_LT_NEG")
        if not usable:
            verdict = "INSUFFICIENT"
        elif pos_gt == len(usable):
            verdict = "CONSISTENT_DIRECTION"
        elif pos_lt == len(usable):
            verdict = "OPPOSITE"
        else:
            verdict = "MIXED"
        out[label] = {"verdict": verdict, "shards": dirs, "n_comparable": len(usable)}
    return out


def pool_hypotheses(per_shard: Dict[str, Any]) -> Dict[str, Any]:
    keys = [
        "H1_buyers_ge_3",
        "H1_complement",
        "H2_buyers_plus_imbalance",
        "H3_price_without_buyers",
        "H4_low_participation",
    ]
    pooled: Dict[str, Any] = {"predeclared": True, "n": 0}
    n_sum = 0
    for row in per_shard.values():
        n_sum += int((row.get("hypotheses") or {}).get("n") or 0)
    pooled["n"] = n_sum
    for k in keys:
        n = 0
        hit2 = 0.0
        hit10 = 0.0
        for row in per_shard.values():
            cell = (row.get("hypotheses") or {}).get(k) or {}
            cn = int(cell.get("n") or 0)
            if cn == 0:
                continue
            n += cn
            if cell.get("p2") is not None:
                hit2 += cell["p2"] * cn
            if cell.get("p10") is not None:
                hit10 += cell["p10"] * cn
        pooled[k] = {
            "n": n,
            "p2": (round(hit2 / n, 6) if n else None),
            "p10": (round(hit10 / n, 6) if n else None),
            "note": "weighted by per-shard n; tokens spanning shards may be double-counted",
        }
    return pooled


def run_parquet_subset(data_dir: Path, out_dir: Path, max_tokens: Optional[int] = None) -> Dict[str, Any]:
    """Stream one trade shard at a time. Does not require the full corpus on disk."""
    import os
    import pyarrow.parquet as pq

    tokens_path = data_dir / "tokens.parquet"
    pf = pq.ParquetFile(tokens_path)
    tokens: List[Dict[str, Any]] = []
    for batch in pf.iter_batches(batch_size=8192, columns=["mint", "detected_at", "creator", "trade_count"]):
        for row in batch.to_pylist():
            det = row.get("detected_at")
            launch_ms = int(det.timestamp() * 1000) if det is not None else 0
            tokens.append(
                {
                    "mint": row["mint"],
                    "launch_ms": launch_ms,
                    "creator": row.get("creator"),
                    "trade_count": int(row.get("trade_count") or 0),
                }
            )
            if max_tokens and len(tokens) >= max_tokens:
                break
        if max_tokens and len(tokens) >= max_tokens:
            break
    launch = {t["mint"]: t["launch_ms"] for t in tokens}
    creators = {t["mint"]: t.get("creator") for t in tokens}
    declared = {t["mint"]: int(t.get("trade_count") or 0) for t in tokens}
    dead_n = sum(1 for t in tokens if int(t.get("trade_count") or 0) == 0)

    trade_dir = data_dir / "trades"
    shards = sorted(trade_dir.glob("trades-*.parquet")) if trade_dir.exists() else []
    out_dir.mkdir(parents=True, exist_ok=True)
    ck_path = out_dir / "SOLANA_SHARD_CHECKPOINT.json"
    ck: Dict[str, Any] = {"done": [], "per_shard": {}, "dataset": "Slinky21/Pumpfun_Memecoin_Corpus"}
    if ck_path.exists():
        try:
            ck = json.loads(ck_path.read_text())
            ck.setdefault("done", [])
            ck.setdefault("per_shard", {})
        except json.JSONDecodeError:
            pass

    pooled_states: Dict[str, MintState] = {}
    feat_path = out_dir / "SOLANA_MOONSHOT_FEATURES.jsonl"
    feat_path.write_text("")

    release = os.environ.get("RELEASE_RAW_SHARDS") == "1"
    processed_shards: List[str] = list(ck.get("done") or [])

    for shard in shards:
        tpf = pq.ParquetFile(shard)
        checksum = shard_checksum(shard)
        shard_states: Dict[str, MintState] = {}
        n_rows = 0
        prev = None
        for batch in tpf.iter_batches(
            batch_size=8192,
            columns=["id", "mint", "event_time", "is_buy", "user_wallet", "price_sol", "sol_amount"],
        ):
            for row in batch.to_pylist():
                mint = row.get("mint")
                if mint not in launch:
                    continue
                et = row.get("event_time")
                age = 0
                event_ms = 0
                if et is not None:
                    event_ms = int(et.timestamp() * 1000)
                    age = max(event_ms - launch[mint], 0)
                rec = {
                    "id": row.get("id") or 0,
                    "mint": mint,
                    "age_ms": age,
                    "is_buy": bool(row.get("is_buy")),
                    "user_wallet": row.get("user_wallet"),
                    "price_sol": row.get("price_sol"),
                    "event_time": event_ms,
                    "sol_amount": row.get("sol_amount"),
                }
                if is_heartbeat(prev, rec):
                    prev = rec
                    continue
                prev = rec
                n_rows += 1
                if mint not in shard_states:
                    shard_states[mint] = MintState(
                        mint=mint,
                        launch_ms=launch[mint],
                        creator=creators.get(mint),
                        trade_count_declared=declared.get(mint, 0),
                    )
                shard_states[mint].observe_trade(
                    rec["age_ms"], rec["is_buy"], rec["user_wallet"], rec["price_sol"]
                )
                if mint not in pooled_states:
                    pooled_states[mint] = MintState(
                        mint=mint,
                        launch_ms=launch[mint],
                        creator=creators.get(mint),
                        trade_count_declared=declared.get(mint, 0),
                    )
                pooled_states[mint].observe_trade(
                    rec["age_ms"], rec["is_buy"], rec["user_wallet"], rec["price_sol"]
                )
        for st in shard_states.values():
            st.finalize_horizons()
        shard_list = list(shard_states.values())
        h = hypothesis_h1_h4(shard_list)
        cohorts = cohort_stats(shard_list)
        ck["per_shard"][shard.name] = {
            "checksum": checksum,
            "row_groups_complete": tpf.num_row_groups,
            "rows": n_rows,
            "tokens_affected": len(shard_states),
            "feature_rows_emitted": len(shard_list) * len(REPORT_HORIZONS),
            "label_rows_emitted": cohorts["population"]["usable_labels"],
            "hypotheses": h,
            "cohorts": cohorts["cohorts"],
            "baseline": cohorts["baseline"],
            "lifts": cohorts["lifts"],
            "pairwise": cohorts["pairwise"],
        }
        if shard.name not in ck["done"]:
            ck["done"].append(shard.name)
        processed_shards = list(ck["done"])
        ck_path.write_text(json.dumps(ck, indent=2) + "\n")
        with feat_path.open("a") as f:
            for st in shard_list:
                o = st.descriptive_outcome()
                f.write(
                    json.dumps(
                        {
                            "chain": "solana",
                            "launchpad": "pumpfun",
                            "token": st.mint,
                            "shard": shard.name,
                            "outcome": o,
                            "features": {k: st.snapshots.get(k) for k in REPORT_HORIZONS},
                        }
                    )
                    + "\n"
                )
        if release:
            try:
                shard.unlink()
            except OSError:
                pass

    for st in pooled_states.values():
        st.finalize_horizons()
    states = list(pooled_states.values())
    stats = cohort_stats(states) if states else cohort_stats([])
    stats["population"]["all_launches"] = len(tokens)
    stats["population"]["zero_trade"] = dead_n
    stats["population"]["active_in_processed_shards"] = len(pooled_states)
    stats["hypotheses"] = hypothesis_h1_h4(states) if states else hypothesis_h1_h4([])
    stats["shards_processed"] = processed_shards
    stats["shards_available_declared"] = 18
    stats["per_shard"] = ck.get("per_shard") or {}
    stats["pooled_hypotheses_from_shards"] = pool_hypotheses(ck.get("per_shard") or {})
    stats["h1_h4_stability"] = direction_consistency(ck.get("per_shard") or {})
    stats["execution_pnl_claimed"] = False
    if len(processed_shards) >= 18:
        stats["shard_limitation"] = "all 18 declared trade shards processed"
        stats["solana_feature_verdict"] = "FULL_CORPUS_COMPLETE"
    elif len(processed_shards) > 1:
        stats["shard_limitation"] = (
            f"processed {len(processed_shards)}/18 local shards; not FULL_CORPUS_COMPLETE"
        )
        stats["solana_feature_verdict"] = "EXPANDED"
    else:
        stats["shard_limitation"] = (
            f"processed {len(processed_shards)}/18 local shards; not FULL_CORPUS_COMPLETE"
        )
        stats["solana_feature_verdict"] = "PARTIAL" if processed_shards else "BLOCKED"
    (out_dir / "SOLANA_MOONSHOT_COHORTS.json").write_text(json.dumps(stats, indent=2) + "\n")
    return stats


def write_outputs(states: List[MintState], out_dir: Path) -> Dict[str, Any]:
    out_dir.mkdir(parents=True, exist_ok=True)
    stats = cohort_stats(states)
    (out_dir / "SOLANA_MOONSHOT_COHORTS.json").write_text(json.dumps(stats, indent=2) + "\n")
    feat_path = out_dir / "SOLANA_MOONSHOT_FEATURES.jsonl"
    with feat_path.open("w") as f:
        for st in states:
            o = st.descriptive_outcome()
            row = {
                "chain": "solana",
                "launchpad": "pumpfun",
                "token": st.mint,
                "outcome": o,
                "features": {k: st.snapshots.get(k) for k in REPORT_HORIZONS},
            }
            f.write(json.dumps(row) + "\n")
    return stats


if __name__ == "__main__":
    import argparse

    p = argparse.ArgumentParser()
    p.add_argument("--data-dir", default="data/pumpfun/Slinky21_Pumpfun_Memecoin_Corpus")
    p.add_argument("--out-dir", default="research")
    p.add_argument("--max-tokens", type=int, default=None)
    args = p.parse_args()
    stats = run_parquet_subset(Path(args.data_dir), Path(args.out_dir), args.max_tokens)
    print(json.dumps({"population": stats["population"], "cohorts": stats["cohorts"]}, indent=2))
