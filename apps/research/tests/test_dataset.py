import unittest
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from memecoin_research.dataset import (
    IMPORTER_VERSION,
    classify_amount,
    dataset_hash,
    dead_token_preserved,
    detect_hour_gaps,
    graduation_bias,
    validate_gate,
)


class DatasetTest(unittest.TestCase):
    def test_manifest_hash_stable(self):
        files = [
            {"path": "b.parquet", "sha256": "bb", "size_bytes": 2},
            {"path": "a.parquet", "sha256": "aa", "size_bytes": 1},
        ]
        h1 = dataset_hash(files, IMPORTER_VERSION, "slinky21-2026-07")
        h2 = dataset_hash(list(reversed(files)), IMPORTER_VERSION, "slinky21-2026-07")
        self.assertEqual(h1, h2)
        h3 = dataset_hash(files, "0.0.0", "slinky21-2026-07")
        self.assertNotEqual(h1, h3)

    def test_classify_amount_no_lamport_invention(self):
        self.assertEqual(classify_amount("123")[0], "ONCHAIN_INTEGER")
        self.assertEqual(classify_amount("0.01")[0], "FLOAT_NOT_INTEGER")
        self.assertEqual(classify_amount(None)[0], "MISSING")

    def test_event_order_same_timestamp_not_random(self):
        rows = [
            {"ts": 1, "mint": "b", "row": 2},
            {"ts": 1, "mint": "a", "row": 1},
            {"ts": 1, "mint": "a", "row": 0},
        ]
        ordered = sorted(rows, key=lambda r: (r["ts"], r["mint"], r["row"]))
        self.assertEqual([r["row"] for r in ordered], [0, 1, 2])

    def test_dedup_counts(self):
        seen = set()
        dups = 0
        for key in [("f", 1), ("f", 1), ("f", 2)]:
            if key in seen:
                dups += 1
            seen.add(key)
        self.assertEqual(dups, 1)

    def test_dead_tokens_preserved(self):
        tokens = [
            {"mint": "a", "graduated_at": None},
            {"mint": "b", "graduated_at": "2026-06-06"},
            {"mint": "c"},
        ]
        self.assertTrue(dead_token_preserved(tokens))
        self.assertFalse(
            dead_token_preserved([{"mint": "x", "graduated_at": "t"}])
        )

    def test_graduation_bias(self):
        self.assertEqual(graduation_bias(100, 100), "GRADUATED_ONLY")
        self.assertEqual(graduation_bias(100, 5), "ALL_LAUNCHES")

    def test_temporal_gaps(self):
        gaps = detect_hour_gaps([10, 11, 20])
        self.assertEqual(len(gaps), 1)
        self.assertEqual(gaps[0]["start"], "12")

    def test_validation_gate_feature_only(self):
        stats = {
            "launches": 10,
            "graduations": 1,
            "dead_tokens_present": True,
            "ordering_valid": True,
            "trade_amounts_valid": False,
            "curve_reconstructable": False,
            "identity_quality": "DERIVED",
        }
        g = validate_gate(stats)
        self.assertTrue(g["feature_valid"])
        self.assertFalse(g["execution_valid"])
        self.assertEqual(g["verdict"], "FEATURE_ONLY")
        self.assertEqual(g["quality_status"], "HISTORICAL_PARTIAL")

    def test_validation_gate_survivor_only_invalid(self):
        g = validate_gate({"launches": 5, "graduations": 5, "dead_tokens_present": False})
        self.assertEqual(g["graduation_bias"], "GRADUATED_ONLY")
        self.assertFalse(g["launch_population_valid"])
        self.assertEqual(g["verdict"], "INVALID")


if __name__ == "__main__":
    unittest.main()
