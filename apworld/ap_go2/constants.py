"""Identity, limits and enums shared by every module. No Archipelago imports."""

GAME_NAME = "Archipela-Go 2: Electric Boogaloo"
ID_OFFSET = 8_902_400_000_000
SCHEMA_VERSION = 4

MAX_TRIPS = 1000
MAX_ZONES = 6
MIN_TIER = 1
MAX_TIER = 10

MODES = ("walk", "run")
DIFFICULTIES = ("easy", "medium", "hard")
GOALS = ("treasure_hunt", "all_trips")
BOSS_GOALS = ("treasure_hunt",)  # goals that add the Boss Quest location

# Player-facing names of the goals, as written in a YAML's goal_selection.
GOAL_NAMES: dict[str, str] = {
    "treasure_hunt": "Treasure Hunt",
    "all_trips": "Completionist",
}

# How the chosen goals combine into winning (slot_data `goal_requirement`).
REQUIREMENTS = ("any", "all", "at_least")

# Inclusive effort-tier band of each difficulty.
DIFFICULTY_BANDS: dict[str, tuple[int, int]] = {
    "easy": (1, 3),
    "medium": (4, 7),
    "hard": (8, 10),
}

# Nominal speeds, km/h, used by clients to turn effort minutes into distance.
MODE_SPEED_KMH: dict[str, float] = {"walk": 4.5, "run": 9.0}

BOSS_FAMILY = "boss"
FAMILY_MODES: dict[str, tuple[str, ...]] = {
    "reach": MODES,
    "dwell": MODES,
    "landmark": MODES,
    "courier": MODES,
    "away": MODES,
    "explore": MODES,
    "trail": MODES,
    "water": MODES,
    "park": MODES,
    "steps": MODES,
}
FAMILIES = tuple(FAMILY_MODES)  # player-selectable families; `boss` is goal-only
REACH_WEIGHT = 3  # `reach` is picked this many times as often as any other family

# Default tool item for each mode that needs one. Walking is always allowed.
MODE_TOOLS: dict[str, str] = {"run": "Running Shoes"}

# Location ID blocks: block = difficulty_index * len(MODES) + mode_index; Boss gets the block after.
BLOCK_SIZE = 1000
BOSS_BLOCK = len(DIFFICULTIES) * len(MODES)
