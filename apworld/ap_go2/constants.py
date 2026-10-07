"""Identity, limits and enums shared by every module. No Archipelago imports."""

GAME_NAME = "Archipela-Go 2: Electric Boogaloo"
ID_OFFSET = 8_902_400_000_000
SCHEMA_VERSION = 2

MAX_TRIPS = 1000
MAX_ZONES = 6
MIN_TIER = 1
MAX_TIER = 10

MODES = ("walk", "run", "bike", "drive")
DIFFICULTIES = ("easy", "medium", "hard")
GOALS = (
    "macguffin_short",
    "macguffin_long",
    "all_trips",
    "boss",
    "treasure_hunt",
    "zone_conqueror",
    "well_rounded",
    "quest_dex",
    "marathon",
    "explorer",
    "streak",
    "boss_rush",
)
BOSS_GOALS = ("boss", "treasure_hunt")  # goals that add the Boss Quest location
MAX_GOAL_TARGET = 1000

# Inclusive effort-tier band of each difficulty.
DIFFICULTY_BANDS: dict[str, tuple[int, int]] = {
    "easy": (1, 3),
    "medium": (4, 7),
    "hard": (8, 10),
}

# Nominal speeds, km/h, used by clients to turn effort minutes into distance.
MODE_SPEED_KMH: dict[str, float] = {"walk": 4.5, "run": 9.0, "bike": 15.0, "drive": 35.0}

BOSS_FAMILY = "boss"
FAMILY_MODES: dict[str, tuple[str, ...]] = {
    "reach": MODES,
    "dwell": MODES,
    "landmark": MODES,
    "courier": MODES,
    "away": MODES,
    "explore": ("walk", "run", "bike"),
    "trail": ("walk", "run", "bike"),
    "water": ("walk", "run", "bike"),
    "park": ("walk", "run"),
    "steps": ("walk", "run"),
}
FAMILIES = tuple(FAMILY_MODES)  # player-selectable families; `boss` is goal-only
REACH_WEIGHT = 3  # `reach` is picked this many times as often as any other family

# Default tool item for each mode that needs one. Walking is always allowed.
MODE_TOOLS: dict[str, str] = {"run": "Running Shoes", "bike": "Bike", "drive": "Car"}

# Location ID blocks: block = difficulty_index * len(MODES) + mode_index; Boss gets the block after.
BLOCK_SIZE = 1000
BOSS_BLOCK = len(DIFFICULTIES) * len(MODES)
