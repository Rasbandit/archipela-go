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


def build(*, boss: bool = True, **over: object) -> dict[str, object]:
    quests = generate_quests(
        rng=random.Random(2),
        zones=2,
        trips=20,
        shares=(50, 35, 15),
        families=FAMILIES,
        boss=boss,
        run_share=25,
    )
    args: dict[str, object] = {
        "goals": ["treasure_hunt" if boss else "all_trips"],
        "goal_requirement": "any",
        "goal_need": 1,
        "minutes_per_tier": 10,
        "reduction_percent": 8,
        "min_distance_m": 150,
        "fog_of_war": False,
        "return_home": False,
        "death_link": True,
        "enabled_traps": ["freeze", "fog"],
        "zones": build_zones(2),
        "quests": quests,
    }
    return build_slot_data(**(args | over))  # type: ignore[arg-type]


def test_output_matches_schema_and_is_json_serializable() -> None:
    data = build()
    jsonschema.validate(json.loads(json.dumps(data)), SCHEMA)
    assert data["schema_version"] == 5


def test_schema_accepts_every_goal() -> None:
    for goal in GOALS:
        jsonschema.validate({**build(), "goals": [goal]}, SCHEMA)


def test_zones_and_boss_shape() -> None:
    data = build()
    assert data["zones"] == [
        {"id": 1, "zone_keys_needed": 0},
        {"id": 2, "zone_keys_needed": 1},
    ]
    boss = data["boss"]
    assert isinstance(boss, dict)
    assert boss["location_id"] == ID_OFFSET + 6001
    assert (boss["zone"], boss["mode"], boss["difficulty"], boss["effort_tier"], boss["type"]) == (
        2,
        "walk",
        "hard",
        10,
        "boss",
    )
    assert boss["needs"] == []


def test_every_quest_lists_its_needs() -> None:
    trips = cast("list[dict[str, Any]]", build()["trips"])
    runs = [t for t in trips if t["mode"] == "run"]
    assert len(runs) == 5
    assert all(t["needs"] == ["Running Shoes"] for t in runs)
    assert all(t["needs"] == [] for t in trips if t["mode"] == "walk")


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
        ("schema_version", 4),
        ("goals", ["nope"]),
        ("goals", [{"id": "treasure_hunt", "target": 0}]),
        ("goals", []),
        ("goal_requirement", "sometimes"),
        ("goal_need", 0),
        ("minutes_per_tier", 0),
    ):
        with pytest.raises(jsonschema.ValidationError):
            jsonschema.validate({**data, key: value}, SCHEMA)
    first_trip = cast("list[dict[str, Any]]", data["trips"])[0]  # JSON-shaped slot_data
    for bad in ({"effort_tier": 11}, {"needs": "Running Shoes"}, {"needs": [""]}):
        broken = {**data, "trips": [{**first_trip, **bad}]}
        with pytest.raises(jsonschema.ValidationError):
            jsonschema.validate(broken, SCHEMA)
    no_needs = {k: v for k, v in first_trip.items() if k != "needs"}
    with pytest.raises(jsonschema.ValidationError):
        jsonschema.validate({**data, "trips": [no_needs]}, SCHEMA)
    with pytest.raises(jsonschema.ValidationError):
        jsonschema.validate(
            {**data, "zones": [{"id": 1, "zone_keys_needed": 0, "tool": None}]}, SCHEMA
        )


def test_committed_sample_matches_schema() -> None:
    sample = json.loads((Path(__file__).parents[1] / "docs" / "slot_data.sample.json").read_text())
    jsonschema.validate(sample, SCHEMA)
    assert sample["boss"] is not None
    assert [z["id"] for z in sample["zones"]] == [1, 2, 3]
    assert len(sample["trips"]) == 60
    assert any(t["needs"] == ["Running Shoes"] for t in sample["trips"])


def test_several_goals_keep_their_order() -> None:
    data = build(
        goals=["treasure_hunt", "all_trips"],
        goal_requirement="at_least",
        goal_need=2,
    )
    jsonschema.validate(data, SCHEMA)
    assert data["goals"] == ["treasure_hunt", "all_trips"]
    assert (data["goal_requirement"], data["goal_need"]) == ("at_least", 2)
