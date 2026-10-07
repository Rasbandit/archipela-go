"""Name constants and name builders. No Archipelago imports."""

from .constants import MODE_TOOLS

ZONE_KEY = "Progressive Zone Key"
EFFORT_REDUCTION = "Progressive Effort Reduction"
SCOUTING = "Progressive Scouting Distance"
COLLECTION = "Progressive Collection Distance"
TOOLS = tuple(MODE_TOOLS.values())
VICTORY = "Victory"
GOAL_LOCATION = "Goal"
BOSS_LOCATION = "Boss Quest"

# Trap option key -> item names it enables. Order is canonical (slot_data and rng use it).
TRAP_ITEMS: dict[str, tuple[str, ...]] = {
    "freeze": ("Freeze Trap",),
    "fog": ("Fog Of War Trap",),
    "shuffle": ("Shuffle Trap",),
    "silence": ("Silence Trap",),
    "leash": ("Leash Trap",),
    "detour": ("Detour Trap",),
    "toll": ("Toll Trap",),
    "slow": ("Slow Trap",),
    "honor": (
        "Push Up Trap",
        "Socializing Trap",
        "Sit Up Trap",
        "Jumping Jack Trap",
        "Touch Grass Trap",
    ),
}
TRAP_KEYS = tuple(TRAP_ITEMS)
APP_TRAPS = tuple(n for key, names in TRAP_ITEMS.items() if key != "honor" for n in names)
HONOR_TRAPS = TRAP_ITEMS["honor"]
ALL_TRAPS = APP_TRAPS + HONOR_TRAPS
FILLERS = ("Hydrate!", "Take a Breather!")

LETTER_NAMES = tuple(f"Letter {c}" for c in "ARCHIPELGO")


def letter(char: str) -> str:
    return f"Letter {char}"


def quest_name(difficulty: str, mode: str, number: int) -> str:
    return f"{difficulty.capitalize()} {mode.capitalize()} Quest #{number}"


def zone_name(zone: int) -> str:
    return f"Zone {zone}"
