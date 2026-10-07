"""Zone access: which keys and tools open each zone. Pure."""

from collections.abc import Sequence
from dataclasses import dataclass

from .constants import MODE_TOOLS


@dataclass(frozen=True)
class Zone:
    id: int  # 1-based
    mode: str
    keys_needed: int  # Progressive Zone Keys to enter (zone 1 is free)
    tool: str | None  # tool item to enter, if the mode differs from zone 1's and needs one


def build_zones(zone_modes: Sequence[str]) -> list[Zone]:
    first = zone_modes[0]
    return [
        Zone(
            id=k,
            mode=mode,
            keys_needed=k - 1,
            tool=MODE_TOOLS.get(mode) if k > 1 and mode != first else None,
        )
        for k, mode in enumerate(zone_modes, start=1)
    ]


def required_tools(zone_modes: Sequence[str]) -> list[str]:
    """Distinct tool items the pool must hold, in order of first use."""
    return list(dict.fromkeys(z.tool for z in build_zones(zone_modes) if z.tool))


def items_to_enter(zones: Sequence[Zone], zone_id: int) -> tuple[int, set[str]]:
    """Keys and tools that must be held together to stand in zone `zone_id`."""
    tools = {z.tool for z in zones if z.id <= zone_id and z.tool}
    return zone_id - 1, tools
