"""Spread quests over zones, difficulties, tiers and families. Pure: takes a seeded Random."""

import random
from collections.abc import Sequence
from dataclasses import dataclass

from . import names
from .constants import BADGES, BOSS_FAMILY, DIFFICULTIES, FAMILY_MODES, MAX_TIER, REACH_WEIGHT
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
    needs: tuple[str, ...] = ()  # tool items that must be held to claim it (its badges)


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


def run_count(trips: int, zones: int, share: int) -> int:
    """Run quests for `share` percent of `trips` (half up, at least 1 when on).

    Capped so the first quest of every zone stays open (walk, no badge).
    """
    if share <= 0:
        return 0
    return min(max(1, (trips * share + 50) // 100), trips - zones)


def generate_quests(  # noqa: PLR0913  # keyword-only, one argument per player option
    *,
    rng: random.Random,
    zones: int,
    trips: int,
    shares: Sequence[int],
    families: Sequence[str],
    boss: bool,
    run_share: int,
) -> QuestPlan:
    """All quests of a seed. Names are numbered per (difficulty, mode) block in creation order.

    Every quest is walk except `run_count` of them, picked at random among all but each zone's
    first quest; a run quest needs its badge (Running Shoes).
    """
    enabled = list(dict.fromkeys(["reach", *families]))
    slots: list[tuple[int, str]] = []
    firsts: set[int] = set()
    for zone, count in enumerate(split_trips(trips, zones), start=1):
        order = [d for d, n in difficulty_counts(count, shares).items() for _ in range(n)]
        rng.shuffle(order)
        firsts.add(len(slots))
        slots += [(zone, d) for d in order]
    candidates = [i for i in range(len(slots)) if i not in firsts]
    runs = set(rng.sample(candidates, run_count(trips, zones, run_share)))

    counters: dict[tuple[str, str], int] = {}
    quests: list[Quest] = []
    for i, (zone, difficulty) in enumerate(slots):
        mode = "run" if i in runs else "walk"
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
                needs=(BADGES[mode],) if mode in BADGES else (),
            )
        )
    boss_quest = None
    if boss:
        boss_quest = Quest(
            name=names.BOSS_LOCATION,
            number=1,
            zone=zones,
            mode="walk",
            difficulty="hard",
            tier=MAX_TIER,
            family=BOSS_FAMILY,
        )
    return QuestPlan(trips=quests, boss=boss_quest)
