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
from worlds.AutoWorld import WebWorld, World  # type: ignore[import-not-found]
from worlds.generic.Rules import set_rule  # type: ignore[import-not-found]

from . import names
from .constants import BOSS_GOALS, GAME_NAME, GOAL_TARGET_OPTIONS, LETTER_GOALS
from .distribution import QuestPlan, generate_quests
from .item_plan import ItemPlan, plan_items
from .items import ITEM_NAME_GROUPS, ITEM_NAME_TO_ID, ITEM_TABLE
from .locations import LOCATION_NAME_GROUPS, LOCATION_NAME_TO_ID
from .options import ApGo2Options, option_groups
from .slot_data import build_slot_data
from .validation import (
    check_requirement,
    goal_ids_from_selection,
    letters_needed_by_logic,
    validate_settings,
)
from .zones import Zone, build_zones


class ApGo2Item(Item):  # type: ignore[misc]
    """An item of this game; only tags the game name for Archipelago."""

    game = GAME_NAME


class ApGo2Location(Location):  # type: ignore[misc]
    """A location of this game; only tags the game name for Archipelago."""

    game = GAME_NAME


class ApGo2Web(WebWorld):  # type: ignore[misc]
    """Web pages of the world: only what Archipelago needs from us today (its option groups)."""

    option_groups = option_groups
    rich_text_options_doc = True


class ApGo2World(World):  # type: ignore[misc]
    """Real-world quests are the checks: travel, complete quests, earn items."""

    game = GAME_NAME
    options_dataclass = ApGo2Options
    options: ApGo2Options  # type: ignore[assignment]
    web = ApGo2Web()
    topology_present = False
    item_name_to_id = ITEM_NAME_TO_ID
    location_name_to_id = LOCATION_NAME_TO_ID
    item_name_groups = ITEM_NAME_GROUPS
    location_name_groups = LOCATION_NAME_GROUPS

    zones: list[Zone]
    quests: QuestPlan
    plan: ItemPlan
    trap_keys: list[str]
    goals: list[str]
    goal_requirement: str
    goal_need: int

    def generate_early(self) -> None:
        """Validate options, then roll zones, quests and the item plan with the seeded RNG.

        Raises:
            OptionError: If the chosen settings cannot produce a valid game.
        """
        opts = self.options
        self.trap_keys = [k for k in names.TRAP_KEYS if k in opts.enabled_traps.value]
        shares = (opts.easy_share.value, opts.medium_share.value, opts.hard_share.value)
        try:
            self.goals = goal_ids_from_selection(opts.goal_selection.value)
            self.goal_requirement = {0: "any", 1: "all", 2: "at_least"}[opts.goal_requirement.value]
            check_requirement(self.goals, self.goal_requirement, opts.goals_required.value)
            self.goal_need = {
                "any": 1,
                "all": len(self.goals),
                "at_least": opts.goals_required.value,
            }[self.goal_requirement]
            modes = validate_settings(
                goal=self.goals,
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
            boss=any(g in BOSS_GOALS for g in self.goals),
        )
        locations = len(self.quests.trips) + (1 if self.quests.boss else 0)
        self.plan = plan_items(
            rng=self.random,
            locations=locations,
            goal=self.goals,
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
        """Chain one region per zone behind Menu, place quests, and lock Victory on the Goal."""
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

        # Letters are demanded only when unavoidable (see letters_needed_by_logic).
        letters = letters_needed_by_logic(self.goals, self.goal_requirement)
        home = menu if all(g in LETTER_GOALS for g in self.goals) else regions[-1]
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
        """Build the named item with its fixed id and classification."""
        code, classification = ITEM_TABLE[name]
        return ApGo2Item(name, classification, code, self.player)

    def create_items(self) -> None:
        """Add the planned item counts to the multiworld pool."""
        for name, count in self.plan.counts.items():
            self.multiworld.itempool += [self.create_item(name) for _ in range(count)]

    def get_filler_item_name(self) -> str:
        """Return a repeatable filler item, as Archipelago may request unlimited copies."""
        return names.FILLERS[0]

    def fill_slot_data(self) -> dict[str, Any]:
        """Build the slot_data the phone client reads (see the client contract schema)."""
        opts = self.options
        return build_slot_data(
            goals=[
                (
                    gid,
                    getattr(opts, GOAL_TARGET_OPTIONS[gid]).value
                    if gid in GOAL_TARGET_OPTIONS
                    else 0,
                )
                for gid in self.goals
            ],
            goal_requirement=self.goal_requirement,
            goal_need=self.goal_need,
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
