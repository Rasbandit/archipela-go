from BaseClasses import ItemClassification  # type: ignore[import-not-found]

from ap_go2 import names
from ap_go2.constants import ID_OFFSET

_TRAP_IDS = {name: 101 + i for i, name in enumerate(names.ALL_TRAPS)}
_FILLER_IDS = {name: 201 + i for i, name in enumerate(names.FILLERS)}
_LETTER_IDS = {name: 301 + i for i, name in enumerate(names.LETTER_NAMES)}

ITEM_TABLE: dict[str, tuple[int, ItemClassification]] = {
    names.KEY: (ID_OFFSET + 1, ItemClassification.progression),
    names.REDUCTION: (ID_OFFSET + 2, ItemClassification.progression),
    names.SCOUTING: (ID_OFFSET + 3, ItemClassification.useful),
    names.COLLECTION: (ID_OFFSET + 4, ItemClassification.useful),
    **{n: (ID_OFFSET + i, ItemClassification.trap) for n, i in _TRAP_IDS.items()},
    **{n: (ID_OFFSET + i, ItemClassification.filler) for n, i in _FILLER_IDS.items()},
    **{n: (ID_OFFSET + i, ItemClassification.progression) for n, i in _LETTER_IDS.items()},
}

ITEM_NAME_TO_ID: dict[str, int] = {name: code for name, (code, _) in ITEM_TABLE.items()}
