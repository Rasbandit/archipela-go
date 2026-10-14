import pytest
from worlds.ap_go2 import names
from worlds.ap_go2.constants import ID_OFFSET, MAX_TRIPS
from worlds.ap_go2.locations import (
    LOCATION_NAME_GROUPS,
    LOCATION_NAME_TO_ID,
    quest_location_id,
)

# The location id table. core/src/solo.rs pins the same rows; change both together.
ID_TABLE = [
    ("easy", "walk", 1, 1),
    ("easy", "run", 7, 1007),
    ("medium", "walk", 1, 2001),
    ("medium", "run", 3, 3003),
    ("hard", "walk", 5, 4005),
    ("hard", "run", 1000, 6000),
]


def test_total_and_unique_ids() -> None:
    assert len(LOCATION_NAME_TO_ID) == 6 * MAX_TRIPS + 1
    assert len(set(LOCATION_NAME_TO_ID.values())) == len(LOCATION_NAME_TO_ID)
    assert all(v > 0 for v in LOCATION_NAME_TO_ID.values())


@pytest.mark.parametrize(("difficulty", "mode", "number", "offset"), ID_TABLE)
def test_id_table(difficulty: str, mode: str, number: int, offset: int) -> None:
    assert quest_location_id(difficulty, mode, number) == ID_OFFSET + offset
    name = f"{difficulty.title()} {mode.title()} Quest #{number}"
    assert LOCATION_NAME_TO_ID[name] == ID_OFFSET + offset


def test_boss_id() -> None:
    assert LOCATION_NAME_TO_ID[names.BOSS_LOCATION] == ID_OFFSET + 6001


def test_groups() -> None:
    assert set(LOCATION_NAME_GROUPS) == {"Easy", "Medium", "Hard", "Walk", "Run", "Boss"}
    assert "Easy Run Quest #3" in LOCATION_NAME_GROUPS["Easy"]
    assert "Easy Run Quest #3" in LOCATION_NAME_GROUPS["Run"]
    assert "Easy Run Quest #3" not in LOCATION_NAME_GROUPS["Hard"]
    assert LOCATION_NAME_GROUPS["Boss"] == {names.BOSS_LOCATION}
    assert names.BOSS_LOCATION in LOCATION_NAME_GROUPS["Hard"]
    assert all(g <= set(LOCATION_NAME_TO_ID) for g in LOCATION_NAME_GROUPS.values())
