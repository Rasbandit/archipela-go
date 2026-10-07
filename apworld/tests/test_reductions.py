import pytest
from worlds.ap_go2.reductions import (
    expected_reductions,
    logic_reductions,
    reductions_needed,
    tier_step_m,
)


def test_expected_reductions_is_15_percent_with_floor_of_five() -> None:
    assert expected_reductions(100, free_slots=100) == 15
    assert expected_reductions(20, free_slots=100) == 5


def test_expected_reductions_never_exceeds_free_slots() -> None:
    assert expected_reductions(100, free_slots=3) == 3
    assert expected_reductions(1, free_slots=0) == 0


def test_disabled_reductions_make_every_tier_reachable() -> None:
    step = tier_step_m(5000, 8, expected=0)
    assert step == pytest.approx(500)
    for tier in range(1, 11):
        needed = reductions_needed(tier, max_distance_m=5000, step_m=step, reduction_percent=8)
        assert needed == 0  # tier 10 is exactly max_distance: float noise must not demand one


def test_top_tier_needs_exactly_the_expected_count() -> None:
    step = tier_step_m(5000, 8, expected=5)
    needed = reductions_needed(10, max_distance_m=5000, step_m=step, reduction_percent=8)
    assert needed == 5


def test_requirements_are_monotonic_and_bounded() -> None:
    step = tier_step_m(10_000, 12, expected=7)
    needs = [
        reductions_needed(t, max_distance_m=10_000, step_m=step, reduction_percent=12)
        for t in range(1, 11)
    ]
    assert needs == sorted(needs)
    assert needs[0] == 0
    assert max(needs) == 7


@pytest.mark.parametrize("percent", [1, 8, 25])
@pytest.mark.parametrize("expected", [0, 5, 15, 150])
def test_logic_reductions_keep_first_three_tiers_free(percent: int, expected: int) -> None:
    logic = logic_reductions(expected, percent)
    assert 0 <= logic <= expected
    step = tier_step_m(10_000, percent, logic)
    for tier in (1, 2, 3):
        needed = reductions_needed(
            tier, max_distance_m=10_000, step_m=step, reduction_percent=percent
        )
        assert needed == 0


def test_logic_reductions_cap_is_tight_and_step_stays_sane() -> None:
    assert logic_reductions(150, 25) == 4
    assert logic_reductions(150, 8) == 14
    assert logic_reductions(3, 8) == 3  # never exceeds what the pool holds
    assert tier_step_m(100_000, 25, logic_reductions(150, 25)) < 1_000_000
