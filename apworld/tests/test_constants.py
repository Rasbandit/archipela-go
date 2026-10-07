from worlds.ap_go2 import constants


def test_identity_constants() -> None:
    assert constants.GAME_NAME == "Archipela-Go 2: Electric Boogaloo"
    assert constants.ID_OFFSET == 8_902_400_000_000
    assert constants.SCHEMA_VERSION == 3


def test_limits_and_enums() -> None:
    assert constants.MAX_TRIPS == 1000
    assert constants.MAX_ZONES == 6
    assert constants.MODES == ("walk", "run", "bike", "drive")
    assert len(constants.GOALS) == 12
    assert constants.GOALS[0] == "macguffin_short"
    assert constants.BOSS_GOALS == ("boss", "treasure_hunt")
    assert constants.MODE_SPEED_KMH == {"walk": 4.5, "run": 9.0, "bike": 15.0, "drive": 35.0}


def test_family_table_matches_spec() -> None:
    f = constants.FAMILY_MODES
    for family in ("reach", "dwell", "landmark", "courier", "away"):
        assert f[family] == constants.MODES
    assert set(f["explore"]) == set(f["trail"]) == set(f["water"]) == {"walk", "run", "bike"}
    assert set(f["park"]) == set(f["steps"]) == {"walk", "run"}
    assert len(constants.FAMILIES) == 10
    assert "boss" not in constants.FAMILIES
