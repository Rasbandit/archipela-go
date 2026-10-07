"""Effort tiers and difficulty bands. Pure.

A quest costs `tier * minutes_per_tier` active minutes; the tier band is its difficulty.
"""

import math

from .constants import DIFFICULTY_BANDS, MAX_TIER, MIN_TIER, MODE_SPEED_KMH


def tier_range(difficulty: str) -> tuple[int, int]:
    return DIFFICULTY_BANDS[difficulty]


def difficulty_of(tier: int) -> str:
    for name, (low, high) in DIFFICULTY_BANDS.items():
        if low <= tier <= high:
            return name
    msg = f"tier {tier} is outside {MIN_TIER}-{MAX_TIER}"
    raise ValueError(msg)


def tier_for_effort(effort_min: float, minutes_per_tier: int) -> int:
    """`ceil(effort_min / minutes_per_tier)` clamped to the valid tier range."""
    return max(MIN_TIER, min(MAX_TIER, math.ceil(effort_min / minutes_per_tier)))


def effort_minutes(tier: int, minutes_per_tier: int) -> int:
    return tier * minutes_per_tier


def distance_km(tier: int, minutes_per_tier: int, mode: str) -> float:
    """Nominal one-way distance of a tier at the mode's nominal speed."""
    return MODE_SPEED_KMH[mode] * effort_minutes(tier, minutes_per_tier) / 60
