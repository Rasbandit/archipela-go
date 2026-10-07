"""Per-seed trip attributes. Pure: takes a seeded Random, returns plain data."""

import random
from collections.abc import Sequence
from dataclasses import dataclass

from .constants import MAX_DISTANCE_TIER


@dataclass(frozen=True)
class Trip:
    number: int  # 1-based; location name is "Trip #{number}"
    distance_tier: int
    key_needed: int
    mode: str


def effective_locks(locks: int, trips: int) -> int:
    """Keys cannot outnumber half the trips, so every area keeps locations."""
    return min(locks, trips // 2)


def generate_trips(
    rng: random.Random, *, count: int, locks: int, modes: Sequence[str]
) -> list[Trip]:
    """Random trips guaranteeing each key tier 0..locks and each distance tier 1..10 appears."""
    if count < 1:
        msg = "count must be at least 1"
        raise ValueError(msg)
    if not modes:
        msg = "modes must not be empty"
        raise ValueError(msg)

    ordered_modes = sorted(modes)
    tiers_needed = min(MAX_DISTANCE_TIER, count)
    keys_needed = min(locks + 1, count)
    pairs: list[tuple[int, int]] = []
    for i in range(count):
        tier = i + 1 if i < tiers_needed else rng.randint(1, MAX_DISTANCE_TIER)
        key = i if i < keys_needed else rng.randint(0, locks)
        pairs.append((tier, key))
    rng.shuffle(pairs)

    return [
        Trip(number=n, distance_tier=tier, key_needed=key, mode=rng.choice(ordered_modes))
        for n, (tier, key) in enumerate(pairs, start=1)
    ]
