from BaseClasses import ItemClassification  # type: ignore[import-not-found]

from . import names
from .constants import ID_OFFSET

_PROG = ItemClassification.progression
_USEFUL = ItemClassification.useful

ITEM_TABLE: dict[str, tuple[int, ItemClassification]] = {
    names.ZONE_KEY: (ID_OFFSET + 1, _PROG),
    "Running Shoes": (ID_OFFSET + 2, _PROG),
    "Bike": (ID_OFFSET + 3, _PROG),
    "Car": (ID_OFFSET + 4, _PROG),
    names.EFFORT_REDUCTION: (ID_OFFSET + 5, _USEFUL),
    names.SCOUTING: (ID_OFFSET + 6, _USEFUL),
    names.COLLECTION: (ID_OFFSET + 7, _USEFUL),
    **{n: (ID_OFFSET + 101 + i, ItemClassification.trap) for i, n in enumerate(names.ALL_TRAPS)},
    **{n: (ID_OFFSET + 201 + i, ItemClassification.filler) for i, n in enumerate(names.FILLERS)},
    **{n: (ID_OFFSET + 301 + i, _PROG) for i, n in enumerate(names.LETTER_NAMES)},
}

ITEM_NAME_TO_ID: dict[str, int] = {name: code for name, (code, _) in ITEM_TABLE.items()}

ITEM_NAME_GROUPS: dict[str, set[str]] = {
    "Traps": set(names.ALL_TRAPS),
    "App Traps": set(names.APP_TRAPS),
    "Honor Traps": set(names.HONOR_TRAPS),
    "Tools": set(names.TOOLS),
    "Letters": set(names.LETTER_NAMES),
    "Zone Keys": {names.ZONE_KEY},
    "Fillers": set(names.FILLERS),
}
