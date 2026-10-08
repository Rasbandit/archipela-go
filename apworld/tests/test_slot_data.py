import json
import random
from pathlib import Path
from typing import Any, cast

import jsonschema
import pytest
from worlds.ap_go2.constants import FAMILIES, GOALS, ID_OFFSET
from worlds.ap_go2.distribution import generate_quests
from worlds.ap_go2.slot_data import build_slot_data
from worlds.ap_go2.zones import build_zones

SCHEMA = json.loads(
    (Path(__file__).parents[1] / "docs" / "slot_data.schema.json").read_text(encoding="utf-8")
)


def build(
    modes: list[str] | None = None, *, boss: bool = True, **over: object
) -> dict[str, object]:
    modes = modes or ["walk", "bike"]
    quests = generate_quests(
        rng=random.Random(2),
        zone_modes=modes,
        trips=20,
        shares=(50, 35, 15),
        families=FAMILIES,
        boss=boss,
    )
    args: dict[str, object] = {
        "goals": [("boss" if boss else "all_trips", 0)],
        "goal_requirement": "any",
        "goal_need": 1,
        "minutes_per_tier": 10,
        "reduction_percent": 8,
        "min_distance_m": 150,
        "fog_of_war": False,
        "return_home": False,
        "death_link": True,
        "enabled_traps": ["freeze", "fog"],
        "zones": build_zones(modes),
        "quests": quests,
    }
    return build_slot_data(**(args | over))  # type: ignore[arg-type]


def test_output_matches_schema_and_is_json_serializable() -> None:
    data = build()
    jsonschema.validate(json.loads(json.dumps(data)), SCHEMA)
    assert data["schema_version"] == 3


def test_schema_accepts_every_goal() -> None:
    for goal in GOALS:
        jsonschema.validate({**build(), "goals": [{"id": goal, "target": 7}]}, SCHEMA)


def test_zones_and_boss_shape() -> None:
    data = build()
    assert data["zones"] == [
        {"id": 1, "mode": "walk", "zone_keys_needed": 0, "tool": None},
        {"id": 2, "mode": "bike", "zone_keys_needed": 1, "tool": "Bike"},
    ]
    boss = data["boss"]
    assert isinstance(boss, dict)
    assert boss["location_id"] == ID_OFFSET + 12_001
    assert (boss["zone"], boss["mode"], boss["difficulty"], boss["effort_tier"], boss["type"]) == (
        2,
        "bike",
        "hard",
        10,
        "boss",
    )


def test_boss_is_null_without_boss_goal() -> None:
    data = build(boss=False)
    assert data["boss"] is None
    jsonschema.validate(data, SCHEMA)


def test_trip_ids_unique_and_in_blocks() -> None:
    trips = build()["trips"]
    assert isinstance(trips, list)
    entries = cast("list[dict[str, Any]]", trips)  # JSON-shaped slot_data; list checked above
    ids = [t["location_id"] for t in entries]
    assert len(set(ids)) == len(ids) == 20
    for t in entries:
        assert t["location_id"] > ID_OFFSET


def test_schema_rejects_bad_data() -> None:
    data = build()
    for key, value in (
        ("schema_version", 2),
        ("goals", [{"id": "nope", "target": 0}]),
        ("goals", []),
        ("goal_requirement", "sometimes"),
        ("goal_need", 0),
        ("minutes_per_tier", 0),
    ):
        with pytest.raises(jsonschema.ValidationError):
            jsonschema.validate({**data, key: value}, SCHEMA)
    first_trip = cast("list[dict[str, Any]]", data["trips"])[0]  # JSON-shaped slot_data
    broken = {**data, "trips": [{**first_trip, "effort_tier": 11}]}
    with pytest.raises(jsonschema.ValidationError):
        jsonschema.validate(broken, SCHEMA)


def test_committed_sample_matches_schema() -> None:
    sample = json.loads((Path(__file__).parents[1] / "docs" / "slot_data.sample.json").read_text())
    jsonschema.validate(sample, SCHEMA)
    assert sample["boss"] is not None
    assert [z["mode"] for z in sample["zones"]] == ["walk", "bike", "drive"]
    assert len(sample["trips"]) == 60


def test_several_goals_keep_their_order_and_targets() -> None:
    data = build(
        goals=[("quest_dex", 9), ("marathon", 42), ("boss", 0)],
        goal_requirement="at_least",
        goal_need=2,
    )
    jsonschema.validate(data, SCHEMA)
    assert data["goals"] == [
        {"id": "quest_dex", "target": 9},
        {"id": "marathon", "target": 42},
        {"id": "boss", "target": 0},
    ]
    assert (data["goal_requirement"], data["goal_need"]) == ("at_least", 2)
