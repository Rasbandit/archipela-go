"""Decide how many of each item go into the pool. Pure."""

import random
from collections import Counter
from collections.abc import Iterable, Sequence
from dataclasses import dataclass

from . import names
from .validation import goal_letter_counts, mandatory_count
from .zones import required_tools

_EFFORT_SHARE_PERCENT = 15
_EFFORT_MIN = 5
_BONUS_SHARE_PERCENT = 5
_BONUS_MIN = 3


@dataclass(frozen=True)
class ItemPlan:
    """How many copies of each item name go into the pool."""

    counts: dict[str, int]


def _useful_count(locations: int, percent: int, minimum: int, free: int) -> int:
    return min(max(minimum, locations * percent // 100), free)


def plan_items(  # noqa: PLR0913
    *,
    rng: random.Random,
    locations: int,
    goal: str | Iterable[str],
    zone_modes: Sequence[str],
    effort: bool,
    scouting: bool,
    collection: bool,
    trap_rate: int,
    traps: Iterable[str],
) -> ItemPlan:
    """Counts sum to `locations`: mandatory items, optional useful items, traps, then filler."""
    mandatory = mandatory_count(goal, zone_modes)
    if locations < mandatory:
        msg = f"locations ({locations}) cannot hold the {mandatory} mandatory items"
        raise ValueError(msg)

    counts: Counter[str] = Counter(goal_letter_counts(goal))
    if len(zone_modes) > 1:
        counts[names.ZONE_KEY] = len(zone_modes) - 1
    counts.update(dict.fromkeys(required_tools(zone_modes), 1))

    free = locations - mandatory
    for enabled, name, percent, minimum in (
        (effort, names.EFFORT_REDUCTION, _EFFORT_SHARE_PERCENT, _EFFORT_MIN),
        (scouting, names.SCOUTING, _BONUS_SHARE_PERCENT, _BONUS_MIN),
        (collection, names.COLLECTION, _BONUS_SHARE_PERCENT, _BONUS_MIN),
    ):
        if enabled and (n := _useful_count(locations, percent, minimum, free)):
            counts[name] = n
            free -= n

    enabled_traps = set(traps)
    trap_pool = [
        n for key in names.TRAP_KEYS if key in enabled_traps for n in names.TRAP_ITEMS[key]
    ]
    trap_count = free * trap_rate // 100 if trap_pool else 0
    if trap_count:
        counts.update(rng.choices(trap_pool, k=trap_count))
    counts.update(rng.choices(names.FILLERS, k=free - trap_count))
    return ItemPlan(counts=dict(counts))
