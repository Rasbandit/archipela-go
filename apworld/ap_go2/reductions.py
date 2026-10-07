"""Distance-reduction math. Tier t sits at t * step; each reduction scales distance by (1 - p)."""

import math

_EPS = 1e-9
_REDUCTION_SHARE = 0.15
_MIN_EXPECTED = 5


def expected_reductions(trips: int, free_slots: int) -> int:
    """Reductions placed in the pool: 15% of trips (min 5), never more than free item slots."""
    wanted = max(_MIN_EXPECTED, math.floor(_REDUCTION_SHARE * trips))
    return max(0, min(wanted, free_slots))


def tier_step_m(max_distance_m: int, reduction_percent: int, expected: int) -> float:
    """Meters per tier so the top tier needs exactly `expected` reductions to fit max_distance."""
    keep = 1 - reduction_percent / 100
    return max_distance_m / keep**expected / 10


def reductions_needed(
    tier: int, *, max_distance_m: int, step_m: float, reduction_percent: int
) -> int:
    """Reductions required before a trip of `tier` fits within max_distance_m."""
    distance = tier * step_m
    if distance <= max_distance_m * (1 + _EPS):
        return 0
    keep = 1 - reduction_percent / 100
    return math.ceil(math.log(max_distance_m / distance) / math.log(keep) - _EPS)
