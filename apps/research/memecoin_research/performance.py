"""Policy performance helpers. Bootstrap is optional and seeded. No ML."""

from __future__ import annotations

import random
from typing import List


def bootstrap_mean(values: List[float], n: int = 200, seed: int = 1) -> tuple[float, float, float]:
    if not values:
        return (0.0, 0.0, 0.0)
    rng = random.Random(seed)
    means = []
    for _ in range(n):
        sample = [values[rng.randrange(len(values))] for _ in values]
        means.append(sum(sample) / len(sample))
    means.sort()
    return (means[len(means) // 2], means[int(len(means) * 0.05)], means[int(len(means) * 0.95)])
