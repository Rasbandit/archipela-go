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
from .constants import BOSS_GOALS, GAME_NAME
from .distribution import QuestPlan, generate_quests
from .item_plan import ItemPlan, plan_items
from .items import ITEM_NAME_GROUPS, ITEM_NAME_TO_ID, ITEM_TABLE
from .locations import LOCATION_NAME_GROUPS, LOCATION_NAME_TO_ID
from .options import ApGo2Options
from .slot_data import build_slot_data
from .validation import goal_letter_counts, validate_settings
from .zones import Zone, build_zones


class ApGo2Item(Item):  # type: ignore[misc]
    game = GAME_NAME


class ApGo2Location(Location):  # type: ignore[misc]
    game = GAME_NAME


class ApGo2World(World):  # type: ignore[misc]
    """Real-world quests are the checks: travel, complete quests, earn items."""

    game = GAME_NAME
    options_dataclass = ApGo2Options
    options: ApGo2Options  # type: ignore[assignment]
    topology_present = False
    item_name_to_id = ITEM_NAME_TO_ID
    location_name_to_id = LOCATION_NAME_TO_ID
    item_name_groups = ITEM_NAME_GROUPS
    location_name_groups = LOCATION_NAME_GROUPS

    zones: list[Zone]
    quests: QuestPlan
    plan: ItemPlan
    trap_keys: list[str]

    @property
    def goal_name(self) -> str:
        return self.options.goal.current_key

    def generate_early(self) -> None:
        opts = self.options
        self.trap_keys = [k for k in names.TRAP_KEYS if k in opts.enabled_traps.value]
        shares = (opts.easy_share.value, opts.medium_share.value, opts.hard_share.value)
        try:
            modes = validate_settings(
                goal=self.goal_name,
                trips=opts.number_of_trips.value,
                zone_modes=list(opts.zone_modes.value),
                shares=shares,
                families=opts.quest_types.value,
                traps=opts.enabled_traps.value,
            )
        except ValueError as exc:
            msg = f"{self.game} ({self.player_name}): {exc}"
            raise OptionError(msg) from exc

        self.zones = build_zones(modes)
        families = sorted(opts.quest_types.value)
        self.quests = generate_quests(
            rng=self.random,
            zone_modes=modes,
            trips=opts.number_of_trips.value,
            shares=shares,
            families=families,
            boss=self.goal_name in BOSS_GOALS,
        )
        locations = len(self.quests.trips) + (1 if self.quests.boss else 0)
        self.plan = plan_items(
            rng=self.random,
            locations=locations,
            goal=self.goal_name,
            zone_modes=modes,
            effort=bool(opts.enable_effort_reductions),
            scouting=bool(opts.enable_scouting_distance_bonuses),
            collection=bool(opts.enable_collection_distance_bonuses),
            trap_rate=opts.trap_rate.value,
            traps=self.trap_keys,
        )

    def _enter_rule(self, zone: Zone) -> Callable[[CollectionState], bool]:
        keys, tool = zone.keys_needed, zone.tool
        return lambda state: (
            state.has(names.ZONE_KEY, self.player, keys)
            and (tool is None or state.has(tool, self.player))
        )

    def create_regions(self) -> None:
        menu = Region("Menu", self.player, self.multiworld)
        regions = [Region(names.zone_name(z.id), self.player, self.multiworld) for z in self.zones]
        self.multiworld.regions += [menu, *regions]

        menu.connect(regions[0], "Start")
        for prev, region, zone in zip(regions, regions[1:], self.zones[1:], strict=False):
            prev.connect(region, f"Unlock {region.name}", self._enter_rule(zone))

        quests = [*self.quests.trips, *([self.quests.boss] if self.quests.boss else [])]
        for quest in quests:
            region = regions[quest.zone - 1]
            region.locations.append(
                ApGo2Location(self.player, quest.name, LOCATION_NAME_TO_ID[quest.name], region)
            )

        letters = goal_letter_counts(self.goal_name)
        home = menu if self.goal_name.startswith("macguffin") else regions[-1]
        goal = ApGo2Location(self.player, names.GOAL_LOCATION, None, home)
        goal.place_locked_item(
            ApGo2Item(names.VICTORY, ItemClassification.progression, None, self.player)
        )
        if letters:
            set_rule(goal, lambda state: state.has_all_counts(letters, self.player))
        home.locations.append(goal)
        self.multiworld.completion_condition[self.player] = lambda state: state.has(
            names.VICTORY, self.player
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
            goal_target=opts.goal_target.value,
            minutes_per_tier=opts.minutes_per_tier.value,
            reduction_percent=opts.reduction_percent.value,
            min_distance_m=opts.minimum_distance.value,
            fog_of_war=bool(opts.fog_of_war),
            return_home=bool(opts.return_home),
            death_link=bool(opts.death_link),
            enabled_traps=self.trap_keys,
            zones=self.zones,
            quests=self.quests,
        )
