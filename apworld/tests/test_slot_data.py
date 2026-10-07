import json
from pathlib import Path

import jsonschema
import pytest
from worlds.ap_go2.constants import ID_OFFSET
from worlds.ap_go2.slot_data import build_slot_data
from worlds.ap_go2.trips import Trip

SCHEMA = json.loads(
    (Path(__file__).parents[1] / "docs" / "slot_data.schema.json").read_text(encoding="utf-8")
)


def build(**over: object) -> dict[str, object]:
    args: dict[str, object] = {
        "goal": "all_trips",
        "minimum_distance_m": 500,
        "maximum_distance_m": 5000,
        "allowed_modes": ("walk", "bike"),
        "return_home": False,
        "death_link": True,
        "reduction_percent": 8,
        "tier_step_m": 758.6,
        "trips": [Trip(number=1, distance_tier=3, key_needed=1, mode="walk")],
    }
    return build_slot_data(**(args | over))  # type: ignore[arg-type]


def test_output_matches_schema_and_is_json_serializable() -> None:
    data = build()
    jsonschema.validate(data, SCHEMA)
    assert json.loads(json.dumps(data)) == data


def test_trip_fields_and_ids() -> None:
    trip = build()["trips"][0]  # type: ignore[index]
    assert trip == {
        "location_id": ID_OFFSET + 1,
        "type": "reach_point",
        "distance_tier": 3,
        "key_needed": 1,
        "mode": "walk",
    }


def test_modes_sorted_and_schema_version_present() -> None:
    data = build(allowed_modes=("drive", "bike"))
    assert data["allowed_modes"] == ["bike", "drive"]
    assert data["schema_version"] == 1


def test_schema_rejects_unknown_goal_and_extra_keys() -> None:
    with pytest.raises(jsonschema.ValidationError):
        jsonschema.validate(build() | {"goal": "nope"}, SCHEMA)
    with pytest.raises(jsonschema.ValidationError):
        jsonschema.validate(build() | {"surprise": 1}, SCHEMA)
