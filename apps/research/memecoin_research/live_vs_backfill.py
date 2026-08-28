"""Compare live raw_events vs independent eth_getLogs for a Robinhood session.

Identity: (tx_hash, log_index). No private keys. Read-only RPC.
"""

from __future__ import annotations

import json
import os
import subprocess
import urllib.request
from typing import Any, Dict, List, Set, Tuple

PONS = {
    "TokenLaunched": "0x8d4aad4953d0ca700d468f3753aa14432d1b35b43ec6409f051fb6aa43a89607",
    "LaunchSwept": "0xcdb72f157fd3666758a6ce201387ffb52038c7562e4fff352828da1096c4b6b4",
    "PoolGraduated": "0x0a44ef75df69c534f43cd6c1aa3ef8983065fe5fe79ef9e79f6494e6f258c259",
    "CurveBuy": "0xec36bf571f136799e8dc0b0b8bea4b04d8bd3d43de838aab0d5fc21d4cbfc455",
    "CurveSell": "0x8113d738abdcb6b38357e9d53a54a7157861a09031b453651f0fe7fe151f59df",
    "SnipeTaxCharged": "0x3bc39a5562b28f5fe8f36cecabfbaa12bb969acf05717994709225fc412a9934",
}
SPAN = 10


def psql(sql: str) -> str:
    url = os.environ.get("DATABASE_URL") or ""
    if not url:
        raise SystemExit("DATABASE_URL is required")
    r = subprocess.run(
        ["psql", url, "-At", "-c", sql],
        check=True,
        capture_output=True,
        text=True,
    )
    return r.stdout.strip()


def rpc(url: str, method: str, params: Any) -> Any:
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(url, data=body, headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=60) as resp:
        payload = json.loads(resp.read().decode())
    if payload.get("error"):
        raise RuntimeError(payload["error"])
    return payload["result"]


def hex_int(n: int) -> str:
    return hex(n)


def fetch_logs(http: str, topic: str, start: int, end: int) -> List[dict]:
    out: List[dict] = []
    cur = start
    while cur <= end:
        to = min(cur + SPAN - 1, end)
        params = [
            {
                "fromBlock": hex_int(cur),
                "toBlock": hex_int(to),
                "topics": [topic],
            }
        ]
        batch = rpc(http, "eth_getLogs", params) or []
        out.extend(batch)
        cur = to + 1
    return out


def ident(tx: str, idx: Any) -> Tuple[str, int]:
    tx = (tx or "").lower()
    if not tx.startswith("0x"):
        tx = "0x" + tx
    return (tx, int(idx or 0))


def compare(chain: str = "robinhood") -> Dict[str, Any]:
    http = os.environ.get("ROBINHOOD_HTTP_URL") or ""
    if not http:
        raise SystemExit("ROBINHOOD_HTTP_URL is required")
    row = psql(
        "SELECT COALESCE(MIN(start_block),0), COALESCE(MAX(end_block),0), "
        "COALESCE(MIN(block_number),0), COALESCE(MAX(block_number),0) "
        "FROM collection_sessions s "
        f"LEFT JOIN raw_events r ON r.chain='{chain}' "
        f"WHERE s.chain='{chain}' AND s.started_at > NOW() - INTERVAL '6 hours'"
    )
    parts = row.split("|") if row else ["0", "0", "0", "0"]
    start_b = int(parts[0] or 0)
    end_b = int(parts[1] or 0)
    min_raw = int(parts[2] or 0)
    max_raw = int(parts[3] or 0)
    start = min(x for x in (start_b, min_raw) if x > 0) if any(x > 0 for x in (start_b, min_raw)) else 0
    end = max(end_b, max_raw)
    def load_live(sql: str) -> Set[Tuple[str, int]]:
        out: Set[Tuple[str, int]] = set()
        text = psql(sql)
        for line in text.splitlines():
            if not line.strip():
                continue
            tx, idx = (line.split("|", 1) + ["0"])[:2]
            out.add(ident(tx, idx))
        return out

    filt = f"chain='{chain}' AND block_number BETWEEN {start} AND {end}" if start and end else f"chain='{chain}'"
    live_by: Dict[str, Set[Tuple[str, int]]] = {
        "TokenLaunched": load_live(
            f"SELECT tx_hash, COALESCE(log_index,0) FROM token_discovered WHERE {filt} AND launchpad='pons_v2'"
        ),
        "CurveBuy": load_live(
            f"SELECT tx_hash, COALESCE(log_index,0) FROM token_trades WHERE {filt} AND launchpad='pons_v2' AND side='buy'"
        ),
        "CurveSell": load_live(
            f"SELECT tx_hash, COALESCE(log_index,0) FROM token_trades WHERE {filt} AND launchpad='pons_v2' AND side='sell'"
        ),
        "LaunchSwept": load_live(
            f"SELECT tx_hash, COALESCE(log_index,0) FROM lifecycle_events WHERE {filt} AND type IN ('launch_swept','LAUNCH_SWEPT')"
        ),
        "PoolGraduated": load_live(
            f"SELECT tx_hash, COALESCE(log_index,0) FROM lifecycle_events WHERE {filt} AND type IN ('pool_graduated','POOL_GRADUATED','graduated','GRADUATED')"
        ),
        "SnipeTaxCharged": load_live(
            f"SELECT tx_hash, COALESCE(log_index,0) FROM lifecycle_events WHERE {filt} AND type IN ('snipe_tax_charged','SNIPE_TAX_CHARGED')"
        ),
    }
    report: Dict[str, Any] = {
        "chain": chain,
        "start_block": start,
        "end_block": end,
        "events": {},
    }
    for name, topic in PONS.items():
        logs = fetch_logs(http, topic, start, end) if start and end and end >= start else []
        backfill = set()
        for x in logs:
            idx = x.get("logIndex") or "0x0"
            if isinstance(idx, str):
                idx_n = int(idx, 16)
            else:
                idx_n = int(idx)
            backfill.add(ident(x.get("transactionHash"), idx_n))
        used_live = live_by.get(name, set())
        inter = used_live & backfill
        live_only = used_live - backfill
        back_only = backfill - used_live
        denom = len(used_live | backfill)
        match = round(100.0 * len(inter) / denom, 3) if denom else None
        report["events"][name] = {
            "live": len(used_live),
            "backfill": len(backfill),
            "intersection": len(inter),
            "live_only": len(live_only),
            "backfill_only": len(back_only),
            "match_pct": match,
        }
    return report


if __name__ == "__main__":
    print(json.dumps(compare(), indent=2))
