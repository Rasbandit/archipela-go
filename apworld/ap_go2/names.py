"""Name constants. No Archipelago imports."""

KEY = "Progressive Key"
REDUCTION = "Progressive Distance Reduction"
SCOUTING = "Progressive Scouting Distance"
COLLECTION = "Progressive Collection Distance"
VICTORY = "Victory"
GOAL_LOCATION = "Goal"

APP_TRAPS = ("Shuffle Trap", "Silence Trap", "Fog Of War Trap")
HONOR_TRAPS = (
    "Push Up Trap",
    "Socializing Trap",
    "Sit Up Trap",
    "Jumping Jack Trap",
    "Touch Grass Trap",
)
ALL_TRAPS = APP_TRAPS + HONOR_TRAPS
FILLERS = ("Hydrate!", "Take a Breather!")

LETTER_NAMES = tuple(f"Letter {c}" for c in "ARCHIPELGO")


def letter(char: str) -> str:
    return f"Letter {char}"


def trip_name(number: int) -> str:
    return f"Trip #{number}"


def area_name(key: int) -> str:
    return f"Area {key}"
