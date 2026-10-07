import json
from pathlib import Path

import jsonschema
import pytest
from BaseClasses import CollectionState
from Fill import distribute_items_restrictive
from Options import OptionError
from test.bases import WorldTestBase  # type: ignore[import-not-found]
from worlds.ap_go2 import names
from worlds.ap_go2.constants import GAME_NAME, GOALS
from worlds.ap_go2.locations import LOCATION_NAME_GROUPS
from worlds.generic.Rules import exclusion_rules

SCHEMA = json.loads(
    (Path(__file__).parents[2] / "docs" / "slot_data.schema.json").read_text(encoding="utf-8")
)
MULTI = ["walk", "bike", "drive"]


class Base(WorldTestBase):
    game = GAME_NAME
    options: dict = {}  # noqa: RUF012

    def goal_reachable(self, state: CollectionState) -> bool:
        """collect_all_but also collects the Victory event, so test the Goal location itself."""
        return state.can_reach_location(names.GOAL_LOCATION, self.player)

    def zone_reachable(self, state: CollectionState, zone: int) -> bool:
        return state.can_reach_region(names.zone_name(zone), self.player)


class TestDefaultSeed(Base):
    options = {"number_of_trips": 100}  # noqa: RUF012

    def test_goal_needs_all_letters(self) -> None:
        state = CollectionState(self.multiworld)
        assert not self.goal_reachable(state)
        self.collect_all_but(["Letter G", names.VICTORY], state)
        assert not self.goal_reachable(state)
        state.collect(self.get_items_by_name("Letter G")[0])
        assert self.goal_reachable(state)

    def test_pool_equals_locations(self) -> None:
        assert len(self.multiworld.itempool) == 100

    def test_slot_data_valid(self) -> None:
        data = self.world.fill_slot_data()
        jsonschema.validate(data, SCHEMA)
        assert len(data["trips"]) == 100
        assert data["boss"] is None


class TestThreeZonesLong(Base):
    options = {  # noqa: RUF012
        "zone_modes": MULTI,
        "goal": "macguffin_short",
        "number_of_trips": 60,
        "enable_effort_reductions": True,
        "enable_scouting_distance_bonuses": True,
        "enable_collection_distance_bonuses": True,
    }

    def test_every_zone_reachable_with_everything(self) -> None:
        state = self.multiworld.get_all_state(False)
        assert all(self.zone_reachable(state, z) for z in (1, 2, 3))

    def test_zones_gated_by_keys(self) -> None:
        fresh = CollectionState(self.multiworld)
        assert self.zone_reachable(fresh, 1)
        assert not self.zone_reachable(fresh, 2)
        without_keys = CollectionState(self.multiworld)
        self.collect_all_but([names.ZONE_KEY, names.VICTORY], without_keys)
        assert not self.zone_reachable(without_keys, 2)
        one_key = CollectionState(self.multiworld)
        self.collect_all_but([names.ZONE_KEY, names.VICTORY], one_key)
        one_key.collect(self.get_items_by_name(names.ZONE_KEY)[0])
        assert self.zone_reachable(one_key, 2)
        assert not self.zone_reachable(one_key, 3)

    def test_zones_gated_by_tools(self) -> None:
        for tool, zone in (("Bike", 2), ("Car", 3)):
            state = CollectionState(self.multiworld)
            self.collect_all_but([tool, names.VICTORY], state)
            assert not self.zone_reachable(state, zone)
            state.collect(self.get_items_by_name(tool)[0])
            assert self.zone_reachable(state, zone)

    def test_no_walk_or_run_tools(self) -> None:
        assert not self.get_items_by_name("Running Shoes")

    def test_slot_data(self) -> None:
        data = self.world.fill_slot_data()
        jsonschema.validate(data, SCHEMA)
        assert [z["tool"] for z in data["zones"]] == [None, "Bike", "Car"]
        assert {t["zone"] for t in data["trips"]} == {1, 2, 3}


class TestBossGoal(Base):
    options = {"zone_modes": ["walk", "bike"], "goal": "boss", "number_of_trips": 30}  # noqa: RUF012

    def test_boss_location_and_pool(self) -> None:
        assert len(self.multiworld.itempool) == 31
        boss = self.multiworld.get_location(names.BOSS_LOCATION, self.player)
        assert boss.parent_region is not None
        assert boss.parent_region.name == "Zone 2"
        assert not self.get_items_by_name("Letter A")

    def test_goal_needs_last_zone(self) -> None:
        assert not self.goal_reachable(CollectionState(self.multiworld))
        assert self.goal_reachable(self.multiworld.get_all_state(False))

    def test_slot_data_boss(self) -> None:
        data = self.world.fill_slot_data()
        jsonschema.validate(data, SCHEMA)
        assert data["boss"]["type"] == "boss"
        assert data["boss"]["effort_tier"] == 10


class TestTreasureHunt(Base):
    options = {"zone_modes": ["walk", "run"], "goal": "treasure_hunt", "number_of_trips": 30}  # noqa: RUF012

    def test_letters_and_boss(self) -> None:
        assert len(self.get_items_by_name("Letter A")) == 1
        assert len(self.multiworld.itempool) == 31
        assert self.multiworld.get_location(names.BOSS_LOCATION, self.player)

    def test_needs_letters_and_last_zone(self) -> None:
        state = CollectionState(self.multiworld)
        self.collect_all_but(["Letter O", names.VICTORY], state)
        assert not self.goal_reachable(state)
        state.collect(self.get_items_by_name("Letter O")[0])
        assert self.goal_reachable(state)
        no_key = CollectionState(self.multiworld)
        self.collect_all_but([names.ZONE_KEY, names.VICTORY], no_key)
        assert not self.goal_reachable(no_key)


class TestAllTrips(Base):
    options = {"goal": "all_trips", "zone_modes": ["run", "walk"], "number_of_trips": 20}  # noqa: RUF012

    def test_run_walk_needs_only_key(self) -> None:
        data = self.world.fill_slot_data()
        assert [z["tool"] for z in data["zones"]] == [None, None]
        assert not self.goal_reachable(CollectionState(self.multiworld))
        assert self.goal_reachable(self.multiworld.get_all_state(False))


class TestSingleZone(Base):
    options = {"goal": "all_trips", "number_of_trips": 1}  # noqa: RUF012

    def test_beatable_at_start(self) -> None:
        assert self.goal_reachable(CollectionState(self.multiworld))
        self.collect_all_but([])
        self.assertBeatable(True)


class TestTrapPoolRestricted(Base):
    options = {  # noqa: RUF012
        "enabled_traps": ["freeze"],
        "trap_rate": 100,
        "number_of_trips": 40,
        "goal": "all_trips",
    }

    def test_only_freeze_traps(self) -> None:
        traps = {i.name for i in self.multiworld.itempool if i.trap}
        assert traps == {"Freeze Trap"}
        assert self.world.fill_slot_data()["enabled_traps"] == ["freeze"]


def _make_goal_test(goal: str) -> type:
    class _T(Base):
        options = {"goal": goal, "zone_modes": ["walk", "bike"], "number_of_trips": 40}  # noqa: RUF012

        def test_beatable_and_valid(self) -> None:
            self.collect_all_but([])
            self.assertBeatable(True)
            jsonschema.validate(self.world.fill_slot_data(), SCHEMA)
            assert self.world.fill_slot_data()["goal"] == goal

    _T.__name__ = f"TestGoal_{goal}"
    _T.__qualname__ = _T.__name__
    return _T


globals().update({f"TestGoal_{g}": _make_goal_test(g) for g in GOALS})


class TestExcludeHard(Base):
    options = {"number_of_trips": 80, "exclude_locations": ["Hard"], "zone_modes": MULTI}  # noqa: RUF012

    def test_no_progression_on_hard(self) -> None:
        # WorldTestBase skips Main's option verification (which expands groups) and exclusion step.
        hard = LOCATION_NAME_GROUPS["Hard"]
        present = {loc.name for loc in self.multiworld.get_locations(self.player)} & hard
        exclusion_rules(self.multiworld, self.player, present)
        distribute_items_restrictive(self.multiworld)
        placed = [
            loc
            for loc in self.multiworld.get_locations(self.player)
            if loc.name in hard and loc.item is not None
        ]
        assert placed
        assert not any(loc.item is not None and loc.item.advancement for loc in placed)


class TestEffortReductionsOptional(Base):
    options = {"number_of_trips": 50, "enable_effort_reductions": True}  # noqa: RUF012

    def test_reductions_are_not_logic(self) -> None:
        assert self.get_items_by_name(names.EFFORT_REDUCTION)
        state = CollectionState(self.multiworld)
        assert all(state.can_reach_location(q.name, self.player) for q in self.world.quests.trips)


class TestLargeSeed(Base):
    options = {  # noqa: RUF012
        "number_of_trips": 1000,
        "zone_modes": ["walk", "run", "bike", "drive", "walk", "run"],
        "goal": "macguffin_long",
    }

    def test_generates_and_fills(self) -> None:
        assert len(self.multiworld.itempool) == 1000
        data = self.world.fill_slot_data()
        jsonschema.validate(data, SCHEMA)
        assert len(data["trips"]) == 1000


class TestInvalidSettings(WorldTestBase):
    game = GAME_NAME
    auto_construct = False

    def assert_rejected(self, options: dict, fragment: str) -> None:
        self.options = options
        with pytest.raises(OptionError, match=fragment):
            self.world_setup()

    def test_empty_zone_modes(self) -> None:
        self.assert_rejected({"zone_modes": []}, "zone_modes")

    def test_unknown_mode(self) -> None:
        self.assert_rejected({"zone_modes": ["swim"]}, "zone_modes")

    def test_too_many_zones(self) -> None:
        self.assert_rejected({"zone_modes": ["walk"] * 7}, "zone_modes")

    def test_all_shares_zero(self) -> None:
        self.assert_rejected({"easy_share": 0, "medium_share": 0, "hard_share": 0}, "share")

    def test_too_few_trips(self) -> None:
        self.assert_rejected({"goal": "macguffin_long", "number_of_trips": 5}, "number_of_trips")
        self.assert_rejected({"zone_modes": MULTI, "number_of_trips": 2}, "number_of_trips")

    def test_unknown_family(self) -> None:
        self.assert_rejected({"quest_types": ["reach", "teleport"]}, "quest_types")

    def test_unknown_trap(self) -> None:
        self.assert_rejected({"enabled_traps": ["boom"]}, "enabled_traps")
