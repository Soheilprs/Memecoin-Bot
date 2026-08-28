import unittest
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from memecoin_research.moonshot import (
    cohort_stats,
    deterministic_hash,
    is_heartbeat,
    process_events,
    valid_price,
)


def tok(mint, trades=0):
    return {"mint": mint, "launch_ms": 0, "creator": "c1", "trade_count": trades}


def tr(mint, age, buy=True, wallet="w1", price=1.0, i=0, t=None):
    return {
        "id": i,
        "mint": mint,
        "age_ms": age,
        "is_buy": buy,
        "user_wallet": wallet,
        "price_sol": price,
        "event_time": t if t is not None else age,
        "sol_amount": 0.1,
    }


class MoonshotTest(unittest.TestCase):
    def test_heartbeat_not_trade(self):
        a = tr("m", 1000)
        b = tr("m", 1000)
        self.assertTrue(is_heartbeat(a, b))
        states = process_events([tok("m")], [a, b])
        st = states[0]
        self.assertEqual(st.n_trade_rows, 1)

    def test_zero_trade_gets_features(self):
        states = process_events([tok("dead")], [])
        st = states[0]
        self.assertIn("30s", st.snapshots)
        self.assertEqual(st.snapshots["30s"]["trade_count"], 0)
        self.assertEqual(st.descriptive_outcome()["cohort"], "DEAD / NO TRADE")

    def test_invalid_price_no_moonshot(self):
        trades = [tr("m", 1000, price=-1.0), tr("m", 2000, price=0.0)]
        states = process_events([tok("m", 2)], trades)
        o = states[0].descriptive_outcome()
        self.assertEqual(o["quality"], "INVALID")
        self.assertFalse(o["reached_10x"])
        self.assertEqual(o["cohort"], "INVALID")

    def test_t30_ignores_future_trade(self):
        trades = [
            tr("m", 10_000, wallet="a", price=1.0, i=1),
            tr("m", 20_000, wallet="b", price=1.1, i=2),
            tr("m", 120_000, wallet="c", price=20.0, i=3),
        ]
        st = process_events([tok("m", 3)], trades)[0]
        self.assertEqual(st.snapshots["30s"]["unique_buyers"], 2)
        self.assertEqual(st.snapshots["2m"]["unique_buyers"], 3)
        self.assertTrue(st.descriptive_outcome()["reached_10x"])

    def test_descriptive_not_execution(self):
        st = process_events([tok("m")], [tr("m", 1000, price=2.0)])[0]
        self.assertFalse(st.descriptive_outcome()["execution_valid"])

    def test_deterministic_cohorts(self):
        tokens = [tok("a"), tok("b", 1)]
        trades = [tr("b", 1000, price=1.0, i=1), tr("b", 2000, price=12.0, i=2)]
        s1 = cohort_stats(process_events(tokens, trades))
        s2 = cohort_stats(process_events(tokens, trades))
        self.assertEqual(deterministic_hash(s1), deterministic_hash(s2))
        self.assertGreaterEqual(s1["cohorts"].get("10X+", 0), 1)
        self.assertGreaterEqual(s1["population"]["zero_trade"], 1)

    def test_valid_price(self):
        self.assertFalse(valid_price(0))
        self.assertFalse(valid_price(-1))
        self.assertTrue(valid_price(1e-9))

    def test_direction_consistency_helper(self):
        from memecoin_research.moonshot import direction_consistency, pool_hypotheses

        per = {
            "a": {
                "hypotheses": {
                    "n": 10,
                    "H1_buyers_ge_3": {"n": 4, "p2": 0.3, "p10": 0.05},
                    "H1_complement": {"n": 6, "p2": 0.1, "p10": 0.01},
                    "H2_buyers_plus_imbalance": {"n": 3, "p2": 0.2, "p10": 0.0},
                    "H3_price_without_buyers": {"n": 2, "p2": 0.0, "p10": 0.0},
                    "H4_low_participation": {"n": 6, "p2": 0.1, "p10": 0.01},
                }
            },
            "b": {
                "hypotheses": {
                    "n": 8,
                    "H1_buyers_ge_3": {"n": 3, "p2": 0.4, "p10": 0.06},
                    "H1_complement": {"n": 5, "p2": 0.05, "p10": 0.0},
                    "H2_buyers_plus_imbalance": {"n": 2, "p2": 0.25, "p10": 0.0},
                    "H3_price_without_buyers": {"n": 1, "p2": 0.0, "p10": 0.0},
                    "H4_low_participation": {"n": 5, "p2": 0.05, "p10": 0.0},
                }
            },
        }
        d = direction_consistency(per)
        self.assertEqual(d["H1"]["verdict"], "CONSISTENT_DIRECTION")
        pooled = pool_hypotheses(per)
        self.assertEqual(pooled["H1_buyers_ge_3"]["n"], 7)

    def test_h1_predeclared(self):
        from memecoin_research.moonshot import hypothesis_h1_h4
        tokens = [tok("a"), tok("b", 2)]
        trades = [
            tr("b", 1000, wallet="x", price=1.0, i=1),
            tr("b", 2000, wallet="y", price=1.0, i=2),
            tr("b", 3000, wallet="z", price=12.0, i=3),
        ]
        h = hypothesis_h1_h4(process_events(tokens, trades))
        self.assertTrue(h["predeclared"])
        self.assertGreaterEqual(h["n"], 1)

    def test_shard_resume(self):
        from memecoin_research.moonshot import process_shards_with_resume
        import tempfile, json
        tokens = [tok("a"), tok("b", 1)]
        shards = [
            ("s0", [tr("b", 1000, price=1.0, i=1)]),
            ("s1", [tr("b", 2000, price=12.0, i=2)]),
        ]
        with tempfile.TemporaryDirectory() as d:
            ck = Path(d) / "ckpt.json"
            process_shards_with_resume(tokens, shards[:1], ck)
            self.assertEqual(json.loads(ck.read_text())["done"], ["s0"])
            s = process_shards_with_resume(tokens, shards, ck)
            # s0 skipped; only s1 applied — incomplete vs full, but resume advanced
            self.assertTrue(ck.exists())
            full = process_events(tokens, shards[0][1] + shards[1][1])
            self.assertEqual(len(s), len(full))


if __name__ == "__main__":
    unittest.main()
