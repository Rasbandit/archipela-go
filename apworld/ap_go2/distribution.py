"""Spread quests over zones, difficulties, tiers and families. Pure: takes a seeded Random."""

import random
from collections.abc import Sequence
from dataclasses import dataclass

from . import names
from .constants import BOSS_FAMILY, DIFFICULTIES, FAMILY_MODES, MAX_TIER, REACH_WEIGHT
from .effort import tier_range


@dataclass(frozen=True)
class Quest:
    """One generated quest location, before it is turned into an Archipelago location."""

    name: str
    number: int  # 1-based within its (difficulty, mode) block; 1 for the boss
    zone: int  # 1-based
    mode: str
    difficulty: str
    tier: int
    family: str


@dataclass(frozen=True)
class QuestPlan:
    """All quests of a game: the regular trips and the optional boss."""

    trips: list[Quest]
    boss: Quest | None


def split_trips(trips: int, zones: int) -> list[int]:
    """Even split; the remainder goes one each to the earliest zones."""
    base, extra = divmod(trips, zones)
    return [base + (1 if k < extra else 0) for k in range(zones)]


def difficulty_counts(count: int, shares: Sequence[int]) -> dict[str, int]:
    """Largest-remainder split of `count` by (easy, medium, hard) shares; ties go to easier."""
    total = sum(shares)
    if total <= 0:
        msg = "at least one difficulty share must be above 0"
        raise ValueError(msg)
    floors = [count * s // total for s in shares]
    remainders = [count * s % total for s in shares]
    order = sorted(range(len(shares)), key=lambda i: (-remainders[i], i))
    for i in order[: count - sum(floors)]:
        floors[i] += 1
    return dict(zip(DIFFICULTIES, floors, strict=True))


def compatible_families(mode: str, enabled: Sequence[str]) -> list[str]:
    """Keep the enabled quest families that can be done in `mode`."""
    return [f for f in enabled if mode in FAMILY_MODES[f]]


def pick_family(rng: random.Random, mode: str, enabled: Sequence[str]) -> str:
    """Pick a family for `mode`; `reach` is weighted up and is the fallback when none fit."""
    pool = compatible_families(mode, enabled)
    if not pool:
        return "reach"
    weights = [REACH_WEIGHT if f == "reach" else 1 for f in pool]
    return rng.choices(pool, weights=weights, k=1)[0]


def generate_quests(  # noqa: PLR0913
    *,
    rng: random.Random,
    zone_modes: Sequence[str],
    trips: int,
    shares: Sequence[int],
    families: Sequence[str],
    boss: bool,
) -> QuestPlan:
    """All quests of a seed. Names are numbered per (difficulty, mode) block in creation order."""
    enabled = list(dict.fromkeys(["reach", *families]))
    counters: dict[tuple[str, str], int] = {}
    quests: list[Quest] = []
    for zone, (mode, count) in enumerate(
        zip(zone_modes, split_trips(trips, len(zone_modes)), strict=True), start=1
    ):
        slots = [d for d, n in difficulty_counts(count, shares).items() for _ in range(n)]
        rng.shuffle(slots)
        for difficulty in slots:
            low, high = tier_range(difficulty)
            number = counters.get((difficulty, mode), 0) + 1
            counters[difficulty, mode] = number
            quests.append(
                Quest(
                    name=names.quest_name(difficulty, mode, number),
                    number=number,
                    zone=zone,
                    mode=mode,
                    difficulty=difficulty,
                    tier=rng.randint(low, high),
                    family=pick_family(rng, mode, enabled),
                )
            )
    boss_quest = None
    if boss:
        last = len(zone_modes)
        boss_quest = Quest(
            name=names.BOSS_LOCATION,
            number=1,
            zone=last,
            mode=zone_modes[-1],
            difficulty="hard",
            tier=MAX_TIER,
            family=BOSS_FAMILY,
        )
    return QuestPlan(trips=quests, boss=boss_quest)
