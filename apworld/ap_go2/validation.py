"""Option validation. Raises ValueError with a player-readable message. Pure."""

from collections import Counter
from collections.abc import Iterable, Sequence

from .constants import FAMILIES, GOAL_NAMES, GOALS, MAX_ZONES, MODES, REQUIREMENTS
from .distribution import split_trips
from .names import TRAP_KEYS, letter
from .zones import build_zones, items_to_enter, required_tools

_LETTERS = {
    "macguffin_short": "APGO",
    "macguffin_long": "ARCHIPELAGO",
    "treasure_hunt": "APGO",
}


def as_goals(goal: str | Iterable[str]) -> tuple[str, ...]:
    """One goal id or several, always as a tuple."""
    return (goal,) if isinstance(goal, str) else tuple(goal)


def goal_letter_counts(goal: str | Iterable[str]) -> dict[str, int]:
    """Letter items the goal(s) need: macguffin goals and treasure_hunt (empty otherwise).

    For several goals the pool must hold what the most demanding one needs: the long word contains
    every letter of the short one, so taking the largest count of each letter is enough.
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


def letters_needed_by_logic(goals: Sequence[str], requirement: str) -> dict[str, int]:
    """Letters the generator may demand before the goal counts as reachable.

    Letters can only be required when finishing them is unavoidable: every selected goal needs
    letters, or all goals are required. Otherwise a player could win another way. (The pool
    still holds them all.)
    """
    letters_are_unavoidable = requirement == "all" or all(g in _LETTERS for g in goals)
    return goal_letter_counts(goals) if letters_are_unavoidable else {}


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


def normalize_zone_modes(values: Iterable[object]) -> list[str]:
    """Lower-case and trim zone modes, checking count and validity.

    Raises:
        ValueError: If there are too few or too many zones, or an unknown mode.
    """
    modes = [str(v).strip().lower() for v in values]
    if not 1 <= len(modes) <= MAX_ZONES:
        msg = f"zone_modes needs 1 to {MAX_ZONES} entries, got {len(modes)}"
        raise ValueError(msg)
    unknown = sorted(set(modes) - set(MODES))
    if unknown:
        msg = f"zone_modes has unknown modes {unknown}; valid: {', '.join(MODES)}"
        raise ValueError(msg)
    return modes


def mandatory_count(goal: str | Iterable[str], zone_modes: Sequence[str]) -> int:
    """Items that must exist: letters, zone keys and tools."""
    return (
        sum(goal_letter_counts(goal).values())
        + len(zone_modes)
        - 1
        + len(required_tools(zone_modes))
    )


def _can_unlock_everything(trips: int, zone_modes: Sequence[str]) -> bool:
    """Every zone has a quest, and the quests already open can hold the items for the next zone."""
    split = split_trips(trips, len(zone_modes))
    if 0 in split:
        return False
    zones = build_zones(zone_modes)
    for k in range(1, len(zones)):
        keys, tools = items_to_enter(zones, k + 1)
        if sum(split[:k]) < keys + len(tools):
            return False
    return True


def min_trips(goal: str | Iterable[str], zone_modes: Sequence[str]) -> int:
    """Smallest `number_of_trips` that holds the mandatory items and keeps every zone unlockable."""
    trips = max(1, mandatory_count(goal, zone_modes))
    while not _can_unlock_everything(trips, zone_modes):
        trips += 1
    return trips


def validate_settings(  # noqa: PLR0913
    *,
    goal: str | Iterable[str],
    trips: int,
    zone_modes: Sequence[str],
    shares: Sequence[int],
    families: Iterable[str],
    traps: Iterable[str],
) -> list[str]:
    """Check everything; return the normalized zone modes."""
    goals = as_goals(goal)
    if not goals:
        msg = "a game needs at least one goal"
        raise ValueError(msg)
    for g in goals:
        if g not in GOALS:
            msg = f"unknown goal {g!r}"
            raise ValueError(msg)
    modes = normalize_zone_modes(zone_modes)
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
    needed = min_trips(goals, modes)
    if trips < needed:
        msg = (
            f"number_of_trips ({trips}) is too small for this goal and these zones; "
            f"it needs at least {needed}"
        )
        raise ValueError(msg)
    return modes
