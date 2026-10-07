import random

import pytest
from worlds.ap_go2 import names
from worlds.ap_go2.item_plan import ItemPlan, plan_items
from worlds.ap_go2.validation import min_trips

ALL_ON = {"effort": True, "scouting": True, "collection": True}


def plan(**over: object) -> ItemPlan:
    args: dict[str, object] = {
        "rng": random.Random(3),
        "locations": 40,
        "goal": "macguffin_short",
        "zone_modes": ["walk", "bike", "drive"],
        "effort": False,
        "scouting": False,
        "collection": False,
        "trap_rate": 50,
        "traps": names.TRAP_KEYS,
    }
    return plan_items(**(args | over))  # type: ignore[arg-type]


def test_pool_size_equals_locations_across_inputs() -> None:
    modes = ["walk", "run", "bike", "drive", "walk", "run"]
    for goal in ("all_trips", "macguffin_short", "macguffin_long", "boss"):
        for locations in (min_trips(goal, modes), 25, 100, 1001):
            for rate in (0, 30, 100):
                result = plan(
                    goal=goal, zone_modes=modes, locations=locations, trap_rate=rate, **ALL_ON
                )
                assert sum(result.counts.values()) == locations
                assert all(v > 0 for v in result.counts.values())


def test_mandatory_items_present() -> None:
    counts = plan().counts
    assert counts[names.ZONE_KEY] == 2
    assert counts["Bike"] == counts["Car"] == 1
    assert "Running Shoes" not in counts
    assert all(counts[names.letter(c)] == 1 for c in "APGO")


def test_boss_goal_has_no_letters() -> None:
    counts = plan(goal="boss").counts
    assert not any(n in counts for n in names.LETTER_NAMES)


def test_single_zone_has_no_keys_or_tools() -> None:
    counts = plan(zone_modes=["bike"], goal="all_trips").counts
    assert names.ZONE_KEY not in counts
    assert not any(t in counts for t in names.TOOLS)


def test_optional_useful_items_only_when_enabled() -> None:
    off = plan(locations=100).counts
    assert not {names.EFFORT_REDUCTION, names.SCOUTING, names.COLLECTION} & set(off)
    on = plan(locations=100, **ALL_ON).counts
    assert {names.EFFORT_REDUCTION, names.SCOUTING, names.COLLECTION} <= set(on)


def test_trap_rate_zero_has_no_traps_and_100_fills_free_slots() -> None:
    assert not set(plan(trap_rate=0).counts) & set(names.ALL_TRAPS)
    full = plan(trap_rate=100, locations=60).counts
    trap_total = sum(v for k, v in full.items() if k in names.ALL_TRAPS)
    assert trap_total == 60 - 2 - 2 - 4
    assert not set(full) & set(names.FILLERS)


def test_trap_pool_restricted() -> None:
    counts = plan(traps=["freeze"], trap_rate=100, locations=60).counts
    assert {k for k in counts if k in names.ALL_TRAPS} == {"Freeze Trap"}


def test_honor_group_expands() -> None:
    counts = plan(traps=["honor"], trap_rate=100, locations=200).counts
    seen = {k for k in counts if k in names.ALL_TRAPS}
    assert seen <= set(names.HONOR_TRAPS)
    assert len(seen) > 1


def test_no_traps_enabled_means_filler_only() -> None:
    counts = plan(traps=[], trap_rate=100).counts
    assert not set(counts) & set(names.ALL_TRAPS)
    assert set(counts) & set(names.FILLERS)


def test_deterministic_for_same_seed() -> None:
    assert plan(rng=random.Random(8), **ALL_ON) == plan(rng=random.Random(8), **ALL_ON)


def test_too_few_locations_raises() -> None:
    with pytest.raises(ValueError, match="mandatory"):
        plan(locations=3)
