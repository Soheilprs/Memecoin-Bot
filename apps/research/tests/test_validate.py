import json
import tempfile
import unittest
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from memecoin_research.load import load_jsonl
from memecoin_research.moonshots import precision_bps, recall_bps
from memecoin_research.outcomes import assert_not_in_features
from memecoin_research.validate import FeatureValidationError, validate_vectors


def vec(**over):
    base = {
        "chain": "solana",
        "token_address": "mint1",
        "as_of_time": "2026-01-01T00:00:30Z",
        "feature_version": "5.0.0",
        "shared": {
            "buy_count_total": 0,
            "holder_count": {"q": "UNKNOWN"},
            "creator_prior_rugs": {"q": "UNKNOWN"},
        },
    }
    base.update(over)
    if "shared" in over:
        shared = dict(base["shared"])
        shared.update(over["shared"])
        base["shared"] = shared
    return base


class ValidateTest(unittest.TestCase):
    def test_unknown_holder_ok(self):
        warnings = validate_vectors([vec()])
        self.assertEqual(warnings, [])

    def test_zero_holder_rejected(self):
        with self.assertRaises(FeatureValidationError):
            validate_vectors([vec(shared={"holder_count": 0, "buy_count_total": 0})])

    def test_no_opportunity_score(self):
        with self.assertRaises(FeatureValidationError):
            validate_vectors([vec(opportunity_score=87)])

    def test_lookahead_time(self):
        a = vec(as_of_time="2026-01-01T00:01:00Z", shared={"buy_count_total": 2})
        b = vec(as_of_time="2026-01-01T00:00:30Z", shared={"buy_count_total": 2})
        with self.assertRaises(FeatureValidationError):
            validate_vectors([a, b])

    def test_jsonl_roundtrip(self):
        rows = [vec()]
        with tempfile.NamedTemporaryFile("w", suffix=".jsonl", delete=False) as f:
            f.write(json.dumps(rows[0]) + "\n")
            path = f.name
        loaded = load_jsonl(path)
        self.assertEqual(loaded[0]["feature_version"], "5.0.0")
        validate_vectors(loaded)
        assert_not_in_features(loaded[0])

    def test_outcome_leak_rejected(self):
        with self.assertRaises(FeatureValidationError):
            assert_not_in_features({"reached_10x": True})

    def test_recall_precision(self):
        self.assertEqual(recall_bps(4, 10), 4000)
        self.assertEqual(precision_bps(2, 8), 2500)
        self.assertIsNone(recall_bps(0, 0))


if __name__ == "__main__":
    unittest.main()
