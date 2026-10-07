"""Client contract v2: build the slot_data dict. Pure and JSON-serializable."""

from collections.abc import Sequence
from typing import Any

from .constants import SCHEMA_VERSION
from .distribution import Quest, QuestPlan
from .locations import location_id
from .zones import Zone


def quest_entry(quest: Quest) -> dict[str, Any]:
    return {
        "location_id": location_id(quest),
        "zone": quest.zone,
        "mode": quest.mode,
        "difficulty": quest.difficulty,
        "effort_tier": quest.tier,
        "type": quest.family,
    }


def build_slot_data(  # noqa: PLR0913
    *,
    goals: Sequence[tuple[str, int]],
    goal_requirement: str,
    goal_need: int,
    minutes_per_tier: int,
    reduction_percent: int,
    min_distance_m: int,
    fog_of_war: bool,
    return_home: bool,
    death_link: bool,
    enabled_traps: Sequence[str],
    zones: Sequence[Zone],
    quests: QuestPlan,
) -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "goals": [{"id": gid, "target": target} for gid, target in goals],
        "goal_requirement": goal_requirement,
        "goal_need": goal_need,
        "minutes_per_tier": minutes_per_tier,
        "reduction_percent": reduction_percent,
        "min_distance_m": min_distance_m,
        "fog_of_war": fog_of_war,
        "return_home": return_home,
        "death_link": death_link,
        "enabled_traps": list(enabled_traps),
        "zones": [
            {"id": z.id, "mode": z.mode, "zone_keys_needed": z.keys_needed, "tool": z.tool}
            for z in zones
        ],
        "trips": [quest_entry(q) for q in quests.trips],
        "boss": quest_entry(quests.boss) if quests.boss else None,
    }
