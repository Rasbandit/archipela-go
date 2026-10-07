from dataclasses import dataclass

from Options import (  # type: ignore[import-not-found]
    Choice,
    DeathLink,
    OptionList,
    OptionSet,
    PerGameCommonOptions,
    Range,
    Toggle,
)

from .constants import FAMILIES, MAX_GOAL_TARGET, MAX_TRIPS
from .names import TRAP_KEYS


class Goal(Choice):
    """Win condition. The last seven are checked by the client once the last zone is reachable."""

    display_name = "Goal"
    option_macguffin_short = 0
    option_macguffin_long = 1
    option_all_trips = 2
    option_boss = 3
    option_treasure_hunt = 4
    option_zone_conqueror = 5
    option_well_rounded = 6
    option_quest_dex = 7
    option_marathon = 8
    option_explorer = 9
    option_streak = 10
    option_boss_rush = 11
    default = 0


class GoalTarget(Range):
    """Target N for goals that count something (0 = the client uses the goal's default)."""

    display_name = "Goal Target"
    range_start = 0
    range_end = MAX_GOAL_TARGET
    default = 0


class NumberOfTrips(Range):
    """Quests across all zones (the boss quest is extra)."""

    display_name = "Number of Trips"
    range_start = 1
    range_end = MAX_TRIPS
    default = 100


class ZoneModes(OptionList):
    """Ordered zones, each walk, run, bike or drive (1 to 6). Zone 1 is free; the rest need keys."""

    display_name = "Zone Modes"
    default = ["walk"]


class EasyShare(Range):
    """Relative share of Easy quests (tiers 1-3)."""

    display_name = "Easy Share"
    range_start = 0
    range_end = 100
    default = 50


class MediumShare(Range):
    """Relative share of Medium quests (tiers 4-7)."""

    display_name = "Medium Share"
    range_start = 0
    range_end = 100
    default = 35


class HardShare(Range):
    """Relative share of Hard quests (tiers 8-10)."""

    display_name = "Hard Share"
    range_start = 0
    range_end = 100
    default = 15


class MinutesPerTier(Range):
    """Active minutes of effort per tier."""

    display_name = "Minutes per Tier"
    range_start = 5
    range_end = 30
    default = 10


class MinimumDistance(Range):
    """Closest a place may be to home, in meters."""

    display_name = "Minimum Distance"
    range_start = 50
    range_end = 5000
    default = 150


class QuestTypes(OptionSet):
    """Enabled quest families (reach is always on). Unknown names are rejected at generation."""

    display_name = "Quest Types"
    default = frozenset(FAMILIES)


class EnabledTraps(OptionSet):
    """Trap pool: freeze, fog, shuffle, silence, leash, detour, toll, slow, honor."""

    display_name = "Enabled Traps"
    default = frozenset(TRAP_KEYS)


class TrapRate(Range):
    """Percent of free item slots that become traps."""

    display_name = "Trap Rate"
    range_start = 0
    range_end = 100
    default = 30


class EnableEffortReductions(Toggle):
    """Add Progressive Effort Reduction items to the pool."""

    display_name = "Enable Effort Reductions"


class EnableScoutingDistanceBonuses(Toggle):
    """Add Progressive Scouting Distance items to the pool."""

    display_name = "Enable Scouting Distance Bonuses"


class EnableCollectionDistanceBonuses(Toggle):
    """Add Progressive Collection Distance items to the pool."""

    display_name = "Enable Collection Distance Bonuses"


class ReductionPercent(Range):
    """Percent each Effort Reduction item shrinks quest effort."""

    display_name = "Reduction Percent"
    range_start = 1
    range_end = 25
    default = 8


class FogOfWar(Toggle):
    """The client hides map points until you get close."""

    display_name = "Fog of War"


class ReturnHome(Toggle):
    """Client requires returning home between quests."""

    display_name = "Return Home"


@dataclass
class ApGo2Options(PerGameCommonOptions):
    goal: Goal
    goal_target: GoalTarget
    number_of_trips: NumberOfTrips
    zone_modes: ZoneModes
    easy_share: EasyShare
    medium_share: MediumShare
    hard_share: HardShare
    minutes_per_tier: MinutesPerTier
    minimum_distance: MinimumDistance
    quest_types: QuestTypes
    enabled_traps: EnabledTraps
    trap_rate: TrapRate
    enable_effort_reductions: EnableEffortReductions
    enable_scouting_distance_bonuses: EnableScoutingDistanceBonuses
    enable_collection_distance_bonuses: EnableCollectionDistanceBonuses
    reduction_percent: ReductionPercent
    fog_of_war: FogOfWar
    return_home: ReturnHome
    death_link: DeathLink
