"""Decide how many of each item go into the pool. Pure."""

import random
from collections import Counter
from dataclasses import dataclass

from ap_go2 import names
from ap_go2.reductions import expected_reductions
from ap_go2.validation import goal_letter_counts


@dataclass(frozen=True)
class ItemPlan:
    counts: dict[str, int]
    expected_reductions: int


def plan_items(  # noqa: PLR0913
    *,
    rng: random.Random,
    trips: int,
    locks: int,
    goal: str,
    reductions_enabled: bool,
    scouting: bool,
    collection: bool,
    trap_rate: int,
) -> ItemPlan:
    """Counts sum to `trips`: letters, keys, reductions, then traps and filler on free slots."""
    counts: Counter[str] = Counter(goal_letter_counts(goal))
    mandatory = sum(counts.values()) + locks
    if trips < mandatory:
        msg = f"trips ({trips}) cannot hold the {mandatory} mandatory items"
        raise ValueError(msg)
    if locks:
        counts[names.KEY] = locks

    free = trips - mandatory
    reductions = expected_reductions(trips, free) if reductions_enabled else 0
    if reductions:
        counts[names.REDUCTION] = reductions
    free -= reductions

    traps = free * trap_rate // 100
    counts.update(rng.choices(names.ALL_TRAPS, k=traps))

    filler_pool = [*names.FILLERS]
    if scouting:
        filler_pool.append(names.SCOUTING)
    if collection:
        filler_pool.append(names.COLLECTION)
    counts.update(rng.choices(filler_pool, k=free - traps))

    return ItemPlan(counts=dict(counts), expected_reductions=reductions)
