import pytest
from worlds.ap_go2.validation import goal_letter_counts, validate_settings


def ok(**over: object) -> dict[str, object]:
    base: dict[str, object] = {
        "goal": "all_trips",
        "trips": 20,
        "locks": 3,
        "min_m": 500,
        "max_m": 5000,
        "modes": ("walk",),
    }
    return base | over


def test_letter_counts() -> None:
    assert goal_letter_counts("all_trips") == {}
    assert goal_letter_counts("macguffin_short") == {
        "Letter A": 1,
        "Letter P": 1,
        "Letter G": 1,
        "Letter O": 1,
    }
    long = goal_letter_counts("macguffin_long")
    assert sum(long.values()) == 11
    assert long["Letter A"] == 2


def test_valid_settings_pass() -> None:
    validate_settings(**ok())  # type: ignore[arg-type]


def test_rejects_min_not_below_max() -> None:
    with pytest.raises(ValueError, match="minimum_distance"):
        validate_settings(**ok(min_m=5000, max_m=5000))  # type: ignore[arg-type]


def test_rejects_empty_modes() -> None:
    with pytest.raises(ValueError, match="allowed_modes"):
        validate_settings(**ok(modes=()))  # type: ignore[arg-type]


def test_rejects_too_few_trips_for_goal() -> None:
    with pytest.raises(ValueError, match="number_of_trips"):
        validate_settings(**ok(goal="macguffin_long", trips=12, locks=3))  # type: ignore[arg-type]


def test_rejects_unknown_goal() -> None:
    with pytest.raises(ValueError, match="goal"):
        validate_settings(**ok(goal="nope"))  # type: ignore[arg-type]
