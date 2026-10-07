import json
from pathlib import Path

import jsonschema
import pytest
from BaseClasses import CollectionState
from Options import OptionError
from test.bases import WorldTestBase  # type: ignore[import-not-found]
from worlds.ap_go2 import names
from worlds.ap_go2.constants import GAME_NAME

SCHEMA = json.loads(
    (Path(__file__).parents[2] / "docs" / "slot_data.schema.json").read_text(encoding="utf-8")
)


class Base(WorldTestBase):
    game = GAME_NAME
    options: dict = {}  # noqa: RUF012

    def goal_reachable(self, state: CollectionState) -> bool:
        """collect_all_but also collects the pre-placed Victory event, so test the Goal rule."""
        return state.can_reach_location(names.GOAL_LOCATION, self.player)


class TestDefaultShortMacguffin(Base):
    options = {"goal": "macguffin_short", "number_of_trips": 20}  # noqa: RUF012

    def test_goal_needs_all_letters(self) -> None:
        state = CollectionState(self.multiworld)
        assert not self.goal_reachable(state)
        self.collect_all_but(["Letter G"], state)
        assert not self.goal_reachable(state)
        state.collect(self.get_items_by_name("Letter G")[0])
        assert self.goal_reachable(state)

    def test_slot_data_matches_schema(self) -> None:
        data = self.world.fill_slot_data()
        jsonschema.validate(data, SCHEMA)
        assert len(data["trips"]) == 20

    def test_item_pool_size_equals_location_count(self) -> None:
        assert len(self.multiworld.itempool) == 20


class TestLongMacguffinWithReductions(Base):
    options = {  # noqa: RUF012
        "goal": "macguffin_long",
        "number_of_trips": 60,
        "number_of_locks": 4,
        "enable_distance_reductions": True,
        "enable_scouting_distance_bonuses": True,
        "enable_collection_distance_bonuses": True,
    }

    def test_all_state_beats_the_game(self) -> None:
        self.collect_all_but([])
        self.assertBeatable(True)

    def test_keys_and_reductions_gate_trips(self) -> None:
        state = CollectionState(self.multiworld)
        for trip in self.world.trips:
            reachable = state.can_reach_location(names.trip_name(trip.number), self.player)
            needs = self.world.reductions_needed_for(trip)
            assert reachable == (trip.key_needed == 0 and needs == 0)


class TestAllTripsGoal(Base):
    options = {"goal": "all_trips", "number_of_trips": 30, "number_of_locks": 3}  # noqa: RUF012

    def test_goal_requires_every_key(self) -> None:
        state = CollectionState(self.multiworld)
        self.collect_all_but([names.KEY], state)
        assert not self.goal_reachable(state)
        for item in self.get_items_by_name(names.KEY):
            state.collect(item)
        assert self.goal_reachable(state)


class TestSingleTrip(Base):
    options = {"goal": "all_trips", "number_of_trips": 1, "number_of_locks": 3}  # noqa: RUF012

    def test_one_trip_world_is_beatable(self) -> None:
        assert self.world.locks == 0
        assert self.goal_reachable(CollectionState(self.multiworld))
        self.assertBeatable(True)


class TestTrapRateExtremes(Base):
    options = {"goal": "macguffin_short", "number_of_trips": 12, "trap_rate": 100}  # noqa: RUF012

    def test_pool_still_matches_locations(self) -> None:
        assert len(self.multiworld.itempool) == 12


class TestOnlyDriveMode(Base):
    options = {"number_of_trips": 25, "allowed_modes": ["drive"]}  # noqa: RUF012

    def test_all_trips_use_drive(self) -> None:
        assert {t["mode"] for t in self.world.fill_slot_data()["trips"]} == {"drive"}


class TestInvalidSettings(WorldTestBase):
    game = GAME_NAME
    auto_construct = False

    def assert_rejected(self, options: dict, fragment: str) -> None:
        self.options = options
        with pytest.raises(OptionError, match=fragment):
            self.world_setup()

    def test_too_few_trips_for_long_goal(self) -> None:
        self.assert_rejected(
            {"goal": "macguffin_long", "number_of_trips": 12, "number_of_locks": 3},
            "number_of_trips",
        )

    def test_minimum_not_below_maximum(self) -> None:
        self.assert_rejected(
            {"minimum_distance": 5000, "maximum_distance": 5000}, "minimum_distance"
        )

    def test_empty_modes(self) -> None:
        self.assert_rejected({"allowed_modes": []}, "allowed_modes")
