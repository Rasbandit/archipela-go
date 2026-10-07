"""Archipela-Go 2: Electric Boogaloo apworld (Archipelago glue)."""

from collections.abc import Callable
from typing import Any

from BaseClasses import (  # type: ignore[import-not-found]
    CollectionState,
    Item,
    ItemClassification,
    Location,
    Region,
)
from Options import OptionError  # type: ignore[import-not-found]
from worlds.AutoWorld import World  # type: ignore[import-not-found]
from worlds.generic.Rules import set_rule  # type: ignore[import-not-found]

from . import names
from .constants import GAME_NAME
from .item_plan import ItemPlan, plan_items
from .items import ITEM_NAME_TO_ID, ITEM_TABLE
from .locations import LOCATION_NAME_TO_ID
from .options import ApGo2Options
from .reductions import logic_reductions, reductions_needed, tier_step_m
from .slot_data import build_slot_data
from .trips import Trip, effective_locks, generate_trips
from .validation import goal_letter_counts, validate_settings

_GOAL_NAMES = {0: "all_trips", 1: "macguffin_short", 2: "macguffin_long"}


class ApGo2Item(Item):  # type: ignore[misc]
    game = GAME_NAME


class ApGo2Location(Location):  # type: ignore[misc]
    game = GAME_NAME


class ApGo2World(World):  # type: ignore[misc]
    """Real-world trips are the checks: travel to places, send locations, earn items."""

    game = GAME_NAME
    options_dataclass = ApGo2Options
    options: ApGo2Options  # type: ignore[assignment]
    topology_present = False
    item_name_to_id = ITEM_NAME_TO_ID
    location_name_to_id = LOCATION_NAME_TO_ID

    trips: list[Trip]
    locks: int
    plan: ItemPlan
    tier_step: float

    @property
    def goal_name(self) -> str:
        return _GOAL_NAMES[self.options.goal.value]

    def generate_early(self) -> None:
        opts = self.options
        modes = sorted(opts.allowed_modes.value)
        try:
            validate_settings(
                goal=self.goal_name,
                trips=opts.number_of_trips.value,
                locks=opts.number_of_locks.value,
                min_m=opts.minimum_distance.value,
                max_m=opts.maximum_distance.value,
                modes=modes,
            )
        except ValueError as exc:
            msg = f"{self.game} ({self.player_name}): {exc}"
            raise OptionError(msg) from exc

        count = opts.number_of_trips.value
        self.locks = effective_locks(opts.number_of_locks.value, count)
        self.plan = plan_items(
            rng=self.random,
            trips=count,
            locks=self.locks,
            goal=self.goal_name,
            reductions_enabled=bool(opts.enable_distance_reductions),
            scouting=bool(opts.enable_scouting_distance_bonuses),
            collection=bool(opts.enable_collection_distance_bonuses),
            trap_rate=opts.trap_rate.value,
        )
        self.tier_step = tier_step_m(
            opts.maximum_distance.value,
            opts.reduction_percent.value,
            logic_reductions(self.plan.expected_reductions, opts.reduction_percent.value),
        )
        self.trips = generate_trips(self.random, count=count, locks=self.locks, modes=modes)

    def reductions_needed_for(self, trip: Trip) -> int:
        return reductions_needed(
            trip.distance_tier,
            max_distance_m=self.options.maximum_distance.value,
            step_m=self.tier_step,
            reduction_percent=self.options.reduction_percent.value,
        )

    def create_regions(self) -> None:
        menu = Region("Menu", self.player, self.multiworld)
        areas = [
            Region(names.area_name(k), self.player, self.multiworld) for k in range(self.locks + 1)
        ]
        self.multiworld.regions += [menu, *areas]

        menu.connect(areas[0], "Start")
        for k in range(1, len(areas)):
            areas[k - 1].connect(
                areas[k],
                f"Unlock {names.area_name(k)}",
                lambda state, k=k: state.has(names.KEY, self.player, k),
            )

        for trip in self.trips:
            name = names.trip_name(trip.number)
            area = areas[trip.key_needed]
            location = ApGo2Location(self.player, name, LOCATION_NAME_TO_ID[name], area)
            need = self.reductions_needed_for(trip)
            if need:
                set_rule(
                    location,
                    lambda state, need=need: state.has(names.REDUCTION, self.player, need),
                )
            area.locations.append(location)

        goal = ApGo2Location(self.player, names.GOAL_LOCATION, None, menu)
        victory = ApGo2Item(names.VICTORY, ItemClassification.progression, None, self.player)
        goal.place_locked_item(victory)
        set_rule(goal, self._goal_rule())
        menu.locations.append(goal)
        self.multiworld.completion_condition[self.player] = lambda state: state.has(
            names.VICTORY, self.player
        )

    def _goal_rule(self) -> Callable[[CollectionState], bool]:
        letters = goal_letter_counts(self.goal_name)
        if letters:
            return lambda state: state.has_all_counts(letters, self.player)
        most = max(self.reductions_needed_for(t) for t in self.trips)
        return lambda state: (
            state.has(names.KEY, self.player, self.locks)
            and state.has(names.REDUCTION, self.player, most)
        )

    def create_item(self, name: str) -> Item:
        code, classification = ITEM_TABLE[name]
        return ApGo2Item(name, classification, code, self.player)

    def create_items(self) -> None:
        for name, count in self.plan.counts.items():
            self.multiworld.itempool += [self.create_item(name) for _ in range(count)]

    def get_filler_item_name(self) -> str:
        return names.FILLERS[0]

    def fill_slot_data(self) -> dict[str, Any]:
        opts = self.options
        return build_slot_data(
            goal=self.goal_name,
            minimum_distance_m=opts.minimum_distance.value,
            maximum_distance_m=opts.maximum_distance.value,
            allowed_modes=sorted(opts.allowed_modes.value),
            return_home=bool(opts.return_home),
            death_link=bool(opts.death_link),
            reduction_percent=opts.reduction_percent.value,
            tier_step_m=self.tier_step,
            trips=self.trips,
        )
