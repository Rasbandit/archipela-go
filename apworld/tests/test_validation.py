import pytest
from worlds.ap_go2.constants import FAMILIES, GOALS
from worlds.ap_go2.names import TRAP_KEYS
from worlds.ap_go2.validation import (
    check_requirement,
    goal_ids_from_selection,
    goal_letter_counts,
    letters_needed_by_logic,
    mandatory_count,
    min_trips,
    normalize_zone_modes,
    validate_settings,
)


def ok(**over: object) -> dict[str, object]:
    base: dict[str, object] = {
        "goal": "all_trips",
        "trips": 20,
        "zone_modes": ["walk"],
        "shares": (50, 35, 15),
        "families": FAMILIES,
        "traps": TRAP_KEYS,
    }
    return base | over


def check(**over: object) -> list[str]:
    return validate_settings(**ok(**over))  # type: ignore[arg-type]


def test_letter_counts() -> None:
    assert goal_letter_counts("all_trips") == {}
    assert goal_letter_counts("boss") == {}
    assert goal_letter_counts("treasure_hunt") == goal_letter_counts("macguffin_short")
    for g in (
        "zone_conqueror",
        "well_rounded",
        "quest_dex",
        "marathon",
        "explorer",
        "streak",
        "boss_rush",
    ):
        assert goal_letter_counts(g) == {}
    assert goal_letter_counts("macguffin_short") == {
        "Letter A": 1,
        "Letter P": 1,
        "Letter G": 1,
        "Letter O": 1,
    }
    long = goal_letter_counts("macguffin_long")
    assert sum(long.values()) == 11
    assert long["Letter A"] == 2


def test_every_goal_validates() -> None:
    for g in GOALS:
        check(goal=g, trips=40)


def test_valid_settings_pass_and_normalize() -> None:
    assert check() == ["walk"]
    assert check(zone_modes=["Walk ", "BIKE"]) == ["walk", "bike"]


@pytest.mark.parametrize("modes", [[], ["walk"] * 7, ["swim"], ["walk", ""]])
def test_rejects_bad_zone_modes(modes: list[str]) -> None:
    with pytest.raises(ValueError, match="zone_modes"):
        check(zone_modes=modes)


def test_accepts_one_to_six_zones() -> None:
    for n in range(1, 7):
        assert len(normalize_zone_modes(["walk"] * n)) == n


def test_rejects_all_zero_shares() -> None:
    with pytest.raises(ValueError, match="share"):
        check(shares=(0, 0, 0))


def test_single_share_is_fine() -> None:
    check(shares=(0, 0, 1))


def test_rejects_unknown_family_and_trap_and_goal() -> None:
    with pytest.raises(ValueError, match="quest_types"):
        check(families=["reach", "teleport"])
    with pytest.raises(ValueError, match="enabled_traps"):
        check(traps=["freeze", "boom"])
    with pytest.raises(ValueError, match="goal"):
        check(goal="nope")


def test_mandatory_counts_keys_tools_letters() -> None:
    assert mandatory_count("all_trips", ["walk"]) == 0
    assert mandatory_count("macguffin_short", ["walk"]) == 4
    # 2 keys + Bike + Car (zone 1 walk)
    assert mandatory_count("all_trips", ["walk", "bike", "drive"]) == 4
    # zone 1 mode needs no tool; walk needs none
    assert mandatory_count("all_trips", ["bike", "bike", "walk"]) == 2
    assert (
        mandatory_count("macguffin_long", ["walk", "run", "bike", "drive", "walk", "run"])
        == 11 + 5 + 3
    )


def test_min_trips_accounts_for_unlock_chain() -> None:
    assert min_trips("all_trips", ["walk"]) == 1
    assert min_trips("macguffin_long", ["walk"]) == 11
    modes = ["walk", "run", "bike", "drive", "walk", "run"]
    n = min_trips("macguffin_long", modes)
    assert n >= 19
    check(goal="macguffin_long", zone_modes=modes, trips=n)
    with pytest.raises(ValueError, match="number_of_trips"):
        check(goal="macguffin_long", zone_modes=modes, trips=n - 1)


def test_rejects_too_few_trips() -> None:
    with pytest.raises(ValueError, match="number_of_trips"):
        check(goal="macguffin_long", trips=10)
    with pytest.raises(ValueError, match="number_of_trips"):
        check(zone_modes=["walk", "bike", "drive"], trips=2)


def test_zones_need_quests_in_each() -> None:
    assert min_trips("all_trips", ["walk", "walk", "walk"]) >= 3


def test_letters_for_several_goals_take_the_most_demanding() -> None:
    both = goal_letter_counts(["macguffin_short", "macguffin_long"])
    assert both == goal_letter_counts("macguffin_long")
    assert goal_letter_counts(["boss", "quest_dex"]) == {}
    assert goal_letter_counts(["treasure_hunt", "boss"]) == goal_letter_counts("macguffin_short")


def test_a_goal_selection_maps_names_to_ids_in_game_order() -> None:
    assert goal_ids_from_selection(["Quest-dex", "letter hunt", "The Big One"]) == [
        "macguffin_short",
        "boss",
        "quest_dex",
    ]


def test_a_goal_selection_must_be_real_and_not_empty() -> None:
    with pytest.raises(ValueError, match="empty"):
        goal_ids_from_selection([])
    with pytest.raises(ValueError, match="Win Instantly"):
        goal_ids_from_selection(["Win Instantly"])


def test_letters_are_logic_only_when_unavoidable() -> None:
    assert letters_needed_by_logic(["macguffin_short"], "any")
    assert letters_needed_by_logic(["macguffin_short", "macguffin_long"], "any")
    assert not letters_needed_by_logic(["macguffin_short", "boss"], "any")
    assert not letters_needed_by_logic(["macguffin_short", "boss"], "at_least")
    assert letters_needed_by_logic(["macguffin_short", "boss"], "all")


def test_the_number_of_goals_required_must_fit() -> None:
    check_requirement(["boss", "quest_dex"], "at_least", 2)
    check_requirement(["boss"], "any", 99)  # unused unless at_least
    with pytest.raises(ValueError, match="goals_required"):
        check_requirement(["boss", "quest_dex"], "at_least", 3)
    with pytest.raises(ValueError, match="requirement"):
        check_requirement(["boss"], "sometimes", 1)


def test_several_goals_are_validated_together() -> None:
    assert check(goal=["boss", "macguffin_long"], trips=40) == ["walk"]
    with pytest.raises(ValueError, match="goal"):
        check(goal=["boss", "nope"])
    with pytest.raises(ValueError, match="at least one goal"):
        check(goal=[])
