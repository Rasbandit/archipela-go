import random

import pytest
from worlds.ap_go2.trips import Trip, effective_locks, generate_trips


def make(count: int, locks: int, modes: tuple[str, ...] = ("walk",), seed: int = 1) -> list[Trip]:
    return generate_trips(random.Random(seed), count=count, locks=locks, modes=modes)


def test_effective_locks_clamps_to_half_of_trips() -> None:
    assert effective_locks(10, 4) == 2
    assert effective_locks(3, 100) == 3
    assert effective_locks(3, 1) == 0


def test_numbers_are_unique_and_sequential() -> None:
    trips = make(50, 3)
    assert sorted(t.number for t in trips) == list(range(1, 51))


def test_every_distance_tier_and_key_tier_is_present() -> None:
    trips = make(30, 4)
    assert {t.distance_tier for t in trips} == set(range(1, 11))
    assert {t.key_needed for t in trips} == set(range(5))


def test_small_counts_still_cover_what_fits() -> None:
    trips = make(3, 1)
    assert len(trips) == 3
    assert 0 in {t.key_needed for t in trips}
    assert {t.distance_tier for t in trips} == {1, 2, 3}


def test_single_trip() -> None:
    (trip,) = make(1, 0)
    assert trip.key_needed == 0
    assert trip.number == 1


def test_modes_come_only_from_allowed_set() -> None:
    trips = make(200, 3, modes=("bike", "drive"))
    assert {t.mode for t in trips} == {"bike", "drive"}


def test_same_seed_is_deterministic() -> None:
    assert make(40, 3, seed=7) == make(40, 3, seed=7)


def test_rejects_empty_modes_and_non_positive_counts() -> None:
    with pytest.raises(ValueError, match="modes"):
        make(5, 1, modes=())
    with pytest.raises(ValueError, match="count"):
        make(0, 0)
