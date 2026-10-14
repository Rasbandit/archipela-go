"""Option validation. Raises ValueError with a player-readable message. Pure."""

from collections import Counter
from collections.abc import Iterable, Sequence

from .constants import FAMILIES, GOAL_NAMES, GOALS, MAX_ZONES, REQUIREMENTS
from .distribution import split_trips
from .names import TRAP_KEYS, letter

_LETTERS = {"treasure_hunt": "APGO"}


def as_goals(goal: str | Iterable[str]) -> tuple[str, ...]:
    """One goal id or several, always as a tuple."""
    return (goal,) if isinstance(goal, str) else tuple(goal)


def goal_letter_counts(goal: str | Iterable[str]) -> dict[str, int]:
    """Letter items the goal(s) need: APGO for treasure_hunt, empty otherwise.

    For several goals the pool holds the largest count of each letter any of them needs.
    """
    need: Counter[str] = Counter()
    for g in as_goals(goal):
        need |= Counter(letter(c) for c in _LETTERS.get(g, ""))
    return dict(need)


def goal_ids_from_selection(selection: Iterable[str]) -> list[str]:
    """Goal ids (in the game's own order) for the display names in a YAML's goal_selection."""
    by_name = {name.casefold(): gid for gid, name in GOAL_NAMES.items()}
    chosen: set[str] = set()
    for raw in selection:
        gid = by_name.get(str(raw).strip().casefold())
        if gid is None:
            valid = ", ".join(GOAL_NAMES.values())
            msg = f"goal_selection has an unknown goal {raw!r}; valid: {valid}"
            raise ValueError(msg)
        chosen.add(gid)
    if not chosen:
        msg = "goal_selection is empty: choose at least one goal"
        raise ValueError(msg)
    return [g for g in GOALS if g in chosen]


def letters_needed_by_logic(goals: Sequence[str], need: int) -> dict[str, int]:
    """Letters the generator may demand before the goal counts as reachable.

    `need` is how many of `goals` must be finished (1 for any, all of them for all, N for at
    least N). Letters are unavoidable when the goals that need no letters cannot make up `need`
    on their own; otherwise a player could win another way. (The pool still holds them all.)
    """
    without_letters = sum(1 for g in goals if g not in _LETTERS)
    return goal_letter_counts(goals) if without_letters < need else {}


def check_requirement(goals: Sequence[str], requirement: str, need: int) -> None:
    """Check the goal requirement mode and its count against the selected goals.

    Raises:
        ValueError: If the mode is unknown or `need` is out of range for `at_least`.
    """
    if requirement not in REQUIREMENTS:
        msg = f"unknown goal requirement {requirement!r}; valid: {', '.join(REQUIREMENTS)}"
        raise ValueError(msg)
    if requirement == "at_least" and not 1 <= need <= len(goals):
        msg = (
            f"goals_required ({need}) must be between 1 and the number of selected goals "
            f"({len(goals)}); "
            "lower it or select more goals"
        )
        raise ValueError(msg)


def mandatory_count(goal: str | Iterable[str], zones: int, tools: Sequence[str]) -> int:
    """Items that must exist: letters, zone keys and tools."""
    return sum(goal_letter_counts(goal).values()) + zones - 1 + len(tools)


def _can_unlock_everything(trips: int, zones: int) -> bool:
    """Every zone has a quest, and the quests before zone k can hold the k - 1 keys to enter it."""
    split = split_trips(trips, zones)
    return 0 not in split and all(sum(split[:k]) >= k for k in range(1, zones))


def min_trips(goal: str | Iterable[str], zones: int, tools: Sequence[str]) -> int:
    """Smallest `number_of_trips` that holds the mandatory items and keeps every zone unlockable."""
    trips = max(1, mandatory_count(goal, zones, tools))
    while not _can_unlock_everything(trips, zones):
        trips += 1
    return trips


def validate_settings(  # noqa: PLR0913  # keyword-only, one argument per player option
    *,
    goal: str | Iterable[str],
    trips: int,
    zones: int,
    tools: Sequence[str],
    shares: Sequence[int],
    families: Iterable[str],
    traps: Iterable[str],
) -> None:
    """Check everything; `tools` are the badge items the pool will hold."""
    goals = as_goals(goal)
    if not goals:
        msg = "a game needs at least one goal"
        raise ValueError(msg)
    for g in goals:
        if g not in GOALS:
            msg = f"unknown goal {g!r}"
            raise ValueError(msg)
    if not 1 <= zones <= MAX_ZONES:
        msg = f"zone_count must be 1 to {MAX_ZONES}, got {zones}"
        raise ValueError(msg)
    if sum(shares) <= 0 or min(shares) < 0:
        msg = "easy_share, medium_share and hard_share cannot all be 0"
        raise ValueError(msg)
    unknown_families = sorted(set(families) - set(FAMILIES))
    if unknown_families:
        msg = f"quest_types has unknown families {unknown_families}; valid: {', '.join(FAMILIES)}"
        raise ValueError(msg)
    unknown_traps = sorted(set(traps) - set(TRAP_KEYS))
    if unknown_traps:
        msg = f"enabled_traps has unknown traps {unknown_traps}; valid: {', '.join(TRAP_KEYS)}"
        raise ValueError(msg)
    needed = min_trips(goals, zones, tools)
    if trips < needed:
        msg = (
            f"number_of_trips ({trips}) is too small for this goal and these zones; "
            f"it needs at least {needed}"
        )
        raise ValueError(msg)
