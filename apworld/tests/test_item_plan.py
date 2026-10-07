import random

import pytest

from ap_go2 import names
from ap_go2.item_plan import ItemPlan, plan_items


def plan(**over: object) -> ItemPlan:
    args: dict[str, object] = {
        "rng": random.Random(3),
        "trips": 20,
        "locks": 3,
        "goal": "macguffin_short",
        "reductions_enabled": False,
        "scouting": False,
        "collection": False,
        "trap_rate": 50,
    }
    return plan_items(**(args | over))  # type: ignore[arg-type]


def test_counts_always_sum_to_trips() -> None:
    for trips in (4, 7, 20, 100, 1000):
        result = plan(trips=trips, locks=min(3, trips // 2 - 2), trap_rate=100)
        assert sum(result.counts.values()) == trips
        assert all(v > 0 for v in result.counts.values())


def test_mandatory_items_present() -> None:
    result = plan()
    assert result.counts[names.KEY] == 3
    assert result.counts["Letter A"] == 1
    assert names.REDUCTION not in result.counts


def test_reductions_use_free_slots_only() -> None:
    result = plan(reductions_enabled=True, trips=20)
    assert result.expected_reductions == 5
    assert result.counts[names.REDUCTION] == 5
    tight = plan(reductions_enabled=True, trips=8, locks=3)  # 8 - 4 letters - 3 keys = 1 free
    assert tight.expected_reductions == 1


def test_trap_rate_zero_has_no_traps_and_hundred_is_all_free_slots() -> None:
    none = plan(trap_rate=0)
    assert not set(none.counts) & set(names.ALL_TRAPS)
    full = plan(trap_rate=100)
    assert sum(v for k, v in full.counts.items() if k in names.ALL_TRAPS) == 20 - 4 - 3


def test_no_free_slots_is_fine() -> None:
    result = plan(trips=7, locks=3, trap_rate=100)  # exactly 4 letters + 3 keys
    assert sum(result.counts.values()) == 7


def test_optional_useful_items_only_when_enabled() -> None:
    off = plan(trap_rate=0)
    assert names.SCOUTING not in off.counts
    assert names.COLLECTION not in off.counts
    on = plan(trap_rate=0, trips=200, scouting=True, collection=True)
    assert names.SCOUTING in on.counts
    assert names.COLLECTION in on.counts


def test_too_few_trips_raises() -> None:
    with pytest.raises(ValueError, match="trips"):
        plan(trips=5, locks=3)
