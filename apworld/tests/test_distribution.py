import random
from collections import Counter

import pytest
from worlds.ap_go2 import names
from worlds.ap_go2.constants import FAMILIES, FAMILY_MODES, MODES
from worlds.ap_go2.distribution import (
    compatible_families,
    difficulty_counts,
    generate_quests,
    pick_family,
    split_trips,
)
from worlds.ap_go2.effort import difficulty_of

SHARES = (50, 35, 15)


def quests(**over: object):
    args: dict[str, object] = {
        "rng": random.Random(1),
        "zone_modes": ["walk"],
        "trips": 100,
        "shares": SHARES,
        "families": FAMILIES,
        "boss": False,
    }
    return generate_quests(**(args | over))  # type: ignore[arg-type]


def test_split_even_with_remainder_to_earliest() -> None:
    assert split_trips(10, 3) == [4, 3, 3]
    assert split_trips(9, 3) == [3, 3, 3]
    assert split_trips(2, 4) == [1, 1, 0, 0]
    assert split_trips(7, 1) == [7]


def test_split_always_sums() -> None:
    for trips in (1, 5, 99, 1000):
        for zones in range(1, 7):
            assert sum(split_trips(trips, zones)) == trips


def test_difficulty_counts_sum_for_many_inputs() -> None:
    rng = random.Random(5)
    for _ in range(300):
        n = rng.randint(0, 400)
        shares = tuple(rng.randint(0, 100) for _ in range(3))
        if not any(shares):
            continue
        assert sum(difficulty_counts(n, shares).values()) == n


def test_difficulty_counts_edge_shares() -> None:
    assert difficulty_counts(10, (100, 0, 0)) == {"easy": 10, "medium": 0, "hard": 0}
    assert difficulty_counts(10, (0, 0, 7)) == {"easy": 0, "medium": 0, "hard": 10}
    assert difficulty_counts(0, SHARES) == {"easy": 0, "medium": 0, "hard": 0}
    assert difficulty_counts(100, SHARES) == {"easy": 50, "medium": 35, "hard": 15}
    assert difficulty_counts(10, (1, 1, 1)) == {"easy": 4, "medium": 3, "hard": 3}


def test_difficulty_counts_rejects_all_zero() -> None:
    with pytest.raises(ValueError, match="share"):
        difficulty_counts(10, (0, 0, 0))


def test_family_compatibility() -> None:
    assert "steps" not in compatible_families("drive", FAMILIES)
    assert "park" not in compatible_families("bike", FAMILIES)
    assert "water" in compatible_families("bike", FAMILIES)
    assert set(compatible_families("walk", FAMILIES)) == set(FAMILIES)
    for mode in MODES:
        assert set(compatible_families(mode, FAMILIES)) == {
            f for f, modes in FAMILY_MODES.items() if mode in modes
        }
    assert compatible_families("drive", ["steps"]) == []


def test_pick_family_falls_back_to_reach_and_respects_mode() -> None:
    rng = random.Random(0)
    assert pick_family(rng, "drive", ["steps", "park"]) == "reach"
    picks = {pick_family(rng, "drive", FAMILIES) for _ in range(500)}
    assert picks <= set(compatible_families("drive", FAMILIES))
    assert "reach" in picks


def test_pick_family_weights_reach_triple() -> None:
    rng = random.Random(0)
    counts = Counter(pick_family(rng, "drive", ["reach", "dwell"]) for _ in range(4000))
    assert 2.5 < counts["reach"] / counts["dwell"] < 3.6


def test_quests_count_difficulty_and_tier_band() -> None:
    result = quests(trips=100)
    assert len(result.trips) == 100
    assert result.boss is None
    for q in result.trips:
        assert difficulty_of(q.tier) == q.difficulty
    assert Counter(q.difficulty for q in result.trips) == {"easy": 50, "medium": 35, "hard": 15}


def test_quests_multi_zone_distribution() -> None:
    result = quests(zone_modes=["walk", "bike", "drive"], trips=100)
    per_zone = Counter(q.zone for q in result.trips)
    assert per_zone == {1: 34, 2: 33, 3: 33}
    for q in result.trips:
        assert q.mode == ["walk", "bike", "drive"][q.zone - 1]
        assert q.family in compatible_families(q.mode, FAMILIES)


def test_quests_all_zone_counts() -> None:
    for zones in range(1, 7):
        modes = [MODES[i % 4] for i in range(zones)]
        result = quests(zone_modes=modes, trips=60)
        assert len(result.trips) == 60
        assert {q.zone for q in result.trips} == set(range(1, zones + 1))


def test_names_unique_and_numbered_per_block() -> None:
    result = quests(zone_modes=["walk", "bike", "walk", "bike"], trips=200)
    all_names = [q.name for q in result.trips]
    assert len(set(all_names)) == 200
    blocks: dict[tuple[str, str], list[int]] = {}
    for q in result.trips:
        blocks.setdefault((q.difficulty, q.mode), []).append(q.number)
    for numbers in blocks.values():
        assert sorted(numbers) == list(range(1, len(numbers) + 1))
    sample = result.trips[0]
    assert sample.name == names.quest_name(sample.difficulty, sample.mode, sample.number)


def test_shares_100_0_0() -> None:
    result = quests(shares=(100, 0, 0), trips=30)
    assert {q.difficulty for q in result.trips} == {"easy"}
    assert {q.tier for q in result.trips} <= {1, 2, 3}


def test_boss_is_hard_tier10_in_last_zone() -> None:
    result = quests(zone_modes=["walk", "bike"], trips=20, boss=True)
    assert len(result.trips) == 20
    boss = result.boss
    assert boss is not None
    assert (boss.zone, boss.mode, boss.difficulty, boss.tier, boss.family) == (
        2,
        "bike",
        "hard",
        10,
        "boss",
    )
    assert boss.name == names.BOSS_LOCATION
    assert boss.name not in {q.name for q in result.trips}


def test_deterministic_for_same_seed() -> None:
    assert quests(rng=random.Random(9)) == quests(rng=random.Random(9))


def test_only_reach_when_no_other_family_enabled() -> None:
    result = quests(families=["reach"])
    assert {q.family for q in result.trips} == {"reach"}
