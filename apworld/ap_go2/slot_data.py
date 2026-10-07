"""Client contract v1: build the slot_data dict. Pure and JSON-serializable."""

from collections.abc import Sequence
from typing import Any

from ap_go2.constants import ID_OFFSET, SCHEMA_VERSION
from ap_go2.trips import Trip


def build_slot_data(  # noqa: PLR0913
    *,
    goal: str,
    minimum_distance_m: int,
    maximum_distance_m: int,
    allowed_modes: Sequence[str],
    return_home: bool,
    death_link: bool,
    reduction_percent: int,
    tier_step_m: float,
    trips: Sequence[Trip],
) -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "goal": goal,
        "min_distance_m": minimum_distance_m,
        "max_distance_m": maximum_distance_m,
        "allowed_modes": sorted(allowed_modes),
        "return_home": return_home,
        "death_link": death_link,
        "reduction_percent": reduction_percent,
        "tier_step_m": tier_step_m,
        "trips": [
            {
                "location_id": ID_OFFSET + t.number,
                "type": "reach_point",
                "distance_tier": t.distance_tier,
                "key_needed": t.key_needed,
                "mode": t.mode,
            }
            for t in trips
        ],
    }
