"""Option validation. Raises ValueError with a player-readable message. Pure."""

from collections import Counter
from collections.abc import Iterable, Sequence

from .constants import FAMILIES, GOALS, MAX_ZONES, MODES
from .distribution import split_trips
from .names import TRAP_KEYS, letter
from .zones import build_zones, items_to_enter, required_tools

_LETTERS = {
    "macguffin_short": "APGO",
    "macguffin_long": "ARCHIPELAGO",
    "treasure_hunt": "APGO",
}


def goal_letter_counts(goal: str) -> dict[str, int]:
    """Letter items the goal needs (macguffin goals and treasure_hunt; empty otherwise)."""
    return dict(Counter(letter(c) for c in _LETTERS.get(goal, "")))


def normalize_zone_modes(values: Iterable[object]) -> list[str]:
    modes = [str(v).strip().lower() for v in values]
    if not 1 <= len(modes) <= MAX_ZONES:
        msg = f"zone_modes needs 1 to {MAX_ZONES} entries, got {len(modes)}"
        raise ValueError(msg)
    unknown = sorted(set(modes) - set(MODES))
    if unknown:
        msg = f"zone_modes has unknown modes {unknown}; valid: {', '.join(MODES)}"
        raise ValueError(msg)
    return modes


def mandatory_count(goal: str, zone_modes: Sequence[str]) -> int:
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


def min_trips(goal: str, zone_modes: Sequence[str]) -> int:
    """Smallest `number_of_trips` that holds the mandatory items and keeps every zone unlockable."""
    trips = max(1, mandatory_count(goal, zone_modes))
    while not _can_unlock_everything(trips, zone_modes):
        trips += 1
    return trips


def validate_settings(  # noqa: PLR0913
    *,
    goal: str,
    trips: int,
    zone_modes: Sequence[str],
    shares: Sequence[int],
    families: Iterable[str],
    traps: Iterable[str],
) -> list[str]:
    """Check everything; return the normalized zone modes."""
    if goal not in GOALS:
        msg = f"unknown goal {goal!r}"
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
    needed = min_trips(goal, modes)
    if trips < needed:
        msg = (
            f"number_of_trips ({trips}) is too small for this goal and these zones; "
            f"it needs at least {needed}"
        )
        raise ValueError(msg)
    return modes
