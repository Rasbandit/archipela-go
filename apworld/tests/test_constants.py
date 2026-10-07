from ap_go2 import constants


def test_identity_constants() -> None:
    assert constants.GAME_NAME == "Archipela-Go 2: Electric Boogaloo"
    assert constants.ID_OFFSET == 8_902_400_000_000
    assert constants.SCHEMA_VERSION == 1


def test_limits_and_enums() -> None:
    assert constants.MAX_TRIPS == 1000
    assert constants.MAX_LOCKS == 10
    assert constants.MAX_DISTANCE_TIER == 10
    assert constants.MODES == ("walk", "bike", "drive")
    assert constants.GOALS == ("all_trips", "macguffin_short", "macguffin_long")
