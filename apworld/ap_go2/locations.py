"""Location ID table and name groups. Pure.

ID = ID_OFFSET + block * 1000 + n, with block = difficulty_index * 4 + mode_index
(difficulty order Easy, Medium, Hard; mode order Walk, Run, Bike, Drive). `Boss Quest` is 12001.
"""

from . import names
from .constants import BLOCK_SIZE, BOSS_BLOCK, DIFFICULTIES, ID_OFFSET, MAX_TRIPS, MODES
from .distribution import Quest


def _block(difficulty: str, mode: str) -> int:
    return DIFFICULTIES.index(difficulty) * len(MODES) + MODES.index(mode)


def quest_location_id(difficulty: str, mode: str, number: int) -> int:
    return ID_OFFSET + _block(difficulty, mode) * BLOCK_SIZE + number


def location_id(quest: Quest) -> int:
    if quest.name == names.BOSS_LOCATION:
        return LOCATION_NAME_TO_ID[names.BOSS_LOCATION]
    return quest_location_id(quest.difficulty, quest.mode, quest.number)


LOCATION_NAME_TO_ID: dict[str, int] = {
    names.quest_name(d, m, n): quest_location_id(d, m, n)
    for d in DIFFICULTIES
    for m in MODES
    for n in range(1, MAX_TRIPS + 1)
}
LOCATION_NAME_TO_ID[names.BOSS_LOCATION] = ID_OFFSET + BOSS_BLOCK * BLOCK_SIZE + 1

LOCATION_NAME_GROUPS: dict[str, set[str]] = {
    **{
        d.capitalize(): {names.quest_name(d, m, n) for m in MODES for n in range(1, MAX_TRIPS + 1)}
        for d in DIFFICULTIES
    },
    **{
        m.capitalize(): {
            names.quest_name(d, m, n) for d in DIFFICULTIES for n in range(1, MAX_TRIPS + 1)
        }
        for m in MODES
    },
    "Boss": {names.BOSS_LOCATION},
}
# The boss is the hardest quest, so excluding "Hard" keeps progression off it too.
LOCATION_NAME_GROUPS["Hard"].add(names.BOSS_LOCATION)
