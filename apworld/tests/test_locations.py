from worlds.ap_go2 import names
from worlds.ap_go2.constants import ID_OFFSET, MAX_TRIPS
from worlds.ap_go2.locations import (
    LOCATION_NAME_GROUPS,
    LOCATION_NAME_TO_ID,
    quest_location_id,
)


def test_total_and_unique_ids() -> None:
    assert len(LOCATION_NAME_TO_ID) == 12 * MAX_TRIPS + 1
    assert len(set(LOCATION_NAME_TO_ID.values())) == len(LOCATION_NAME_TO_ID)
    assert all(v > 0 for v in LOCATION_NAME_TO_ID.values())


def test_id_layout_matches_spec() -> None:
    assert LOCATION_NAME_TO_ID["Easy Walk Quest #1"] == ID_OFFSET + 1
    assert LOCATION_NAME_TO_ID["Easy Run Quest #7"] == ID_OFFSET + 1007
    assert LOCATION_NAME_TO_ID["Medium Walk Quest #1"] == ID_OFFSET + 4001
    assert LOCATION_NAME_TO_ID["Hard Drive Quest #1000"] == ID_OFFSET + 11_000 + 1000
    assert LOCATION_NAME_TO_ID[names.BOSS_LOCATION] == ID_OFFSET + 12_001
    assert quest_location_id("hard", "bike", 5) == ID_OFFSET + 10_005


def test_groups() -> None:
    assert set(LOCATION_NAME_GROUPS) == {
        "Easy", "Medium", "Hard", "Walk", "Run", "Bike", "Drive", "Boss",
    }  # fmt: skip
    assert "Easy Bike Quest #3" in LOCATION_NAME_GROUPS["Easy"]
    assert "Easy Bike Quest #3" in LOCATION_NAME_GROUPS["Bike"]
    assert "Easy Bike Quest #3" not in LOCATION_NAME_GROUPS["Hard"]
    assert LOCATION_NAME_GROUPS["Boss"] == {names.BOSS_LOCATION}
    assert names.BOSS_LOCATION in LOCATION_NAME_GROUPS["Hard"]
    assert all(g <= set(LOCATION_NAME_TO_ID) for g in LOCATION_NAME_GROUPS.values())
