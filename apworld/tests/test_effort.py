import pytest
from worlds.ap_go2.effort import (
    difficulty_of,
    distance_km,
    effort_minutes,
    tier_for_effort,
    tier_range,
)


def test_bands_partition_all_tiers() -> None:
    seen = [difficulty_of(t) for t in range(1, 11)]
    assert seen == ["easy"] * 3 + ["medium"] * 4 + ["hard"] * 3


@pytest.mark.parametrize("tier", [0, 11, -1])
def test_difficulty_of_rejects_out_of_range(tier: int) -> None:
    with pytest.raises(ValueError, match="tier"):
        difficulty_of(tier)


def test_tier_range_matches_difficulty_of() -> None:
    for name in ("easy", "medium", "hard"):
        low, high = tier_range(name)
        assert difficulty_of(low) == difficulty_of(high) == name


def test_tier_for_effort_ceils_and_clamps() -> None:
    assert tier_for_effort(10, 10) == 1
    assert tier_for_effort(10.1, 10) == 2
    assert tier_for_effort(0, 10) == 1
    assert tier_for_effort(10_000, 10) == 10


def test_effort_and_distance() -> None:
    assert effort_minutes(4, 10) == 40
    assert distance_km(6, 10, "walk") == pytest.approx(4.5)
    assert distance_km(6, 10, "drive") == pytest.approx(35.0)
