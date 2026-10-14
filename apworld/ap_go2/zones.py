"""Zones: how many Progressive Zone Keys open each one. Pure."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Zone:
    """One zone and the keys needed to enter it."""

    id: int  # 1-based
    keys_needed: int  # Progressive Zone Keys to enter (zone 1 is free)


def build_zones(count: int) -> list[Zone]:
    """Zones 1..count; zone k needs k - 1 keys."""
    return [Zone(id=k, keys_needed=k - 1) for k in range(1, count + 1)]
