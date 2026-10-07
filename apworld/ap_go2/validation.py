"""Option validation. Raises ValueError with a player-readable message."""

from collections import Counter
from collections.abc import Sequence

from ap_go2.constants import GOALS, MODES
from ap_go2.names import letter
from ap_go2.trips import effective_locks

_LETTERS = {"macguffin_short": "APGO", "macguffin_long": "ARCHIPELAGO"}


def goal_letter_counts(goal: str) -> dict[str, int]:
    """Letter items required to win the macguffin goals (empty for all_trips)."""
    return dict(Counter(letter(c) for c in _LETTERS.get(goal, "")))


def validate_settings(  # noqa: PLR0913
    *, goal: str, trips: int, locks: int, min_m: int, max_m: int, modes: Sequence[str]
) -> None:
    if goal not in GOALS:
        msg = f"unknown goal {goal!r}"
        raise ValueError(msg)
    if not modes:
        msg = "allowed_modes must contain at least one of walk, bike, drive"
        raise ValueError(msg)
    unknown = set(modes) - set(MODES)
    if unknown:
        msg = f"allowed_modes has unknown modes: {sorted(unknown)}"
        raise ValueError(msg)
    if min_m >= max_m:
        msg = f"minimum_distance ({min_m}) must be below maximum_distance ({max_m})"
        raise ValueError(msg)
    needed = sum(goal_letter_counts(goal).values()) + effective_locks(locks, trips)
    if trips < needed:
        msg = f"number_of_trips ({trips}) is too small for this goal; it needs at least {needed}"
        raise ValueError(msg)
