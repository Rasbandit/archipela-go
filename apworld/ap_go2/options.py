"""Player-facing options of the world, plus their web-UI grouping."""

from dataclasses import dataclass

from Options import (  # type: ignore[import-not-found]
    Choice,
    DeathLink,
    OptionGroup,
    OptionList,
    OptionSet,
    PerGameCommonOptions,
    Range,
    Toggle,
)
from schema import And, Schema  # type: ignore[import-not-found]

from .constants import FAMILIES, GOAL_NAMES, MAX_GOAL_TARGET, MAX_TRIPS
from .names import TRAP_KEYS


class GoalSelection(OptionSet):
    """What will be your goal(s)? Choose one or several.

    Configure them further with the other goal options.

    - **Letter Hunt**: collect the letters of APGO.
    - **Letter Hunt XL**: collect the letters of ARCHIPELAGO.
    - **Completionist**: finish every quest.
    - **The Big One**: finish the Boss Quest in the last zone.
    - **Treasure Hunt**: collect APGO, then finish the Boss Quest.
    - **Zone Conqueror**: finish a percentage of the quests in every zone.
    - **Well Rounded**: finish one quest of every enabled type.
    - **Quest-dex**: finish N different kinds of quest.
    - **Marathon**: travel N kilometers on quests.
    - **Explorer**: reveal N map cells.
    - **Daily Habit**: finish a quest on N days in a row.
    - **Boss Rush**: finish N hard quests.

    Every goal except the letter goals is checked by the app once the last zone can be reached.
    """

    display_name = "Select your Goals"
    rich_text_doc = True
    valid_keys = frozenset(GOAL_NAMES.values())
    default = frozenset({GOAL_NAMES["macguffin_short"]})
    schema = Schema(And(set, len), error="goal_selection is empty: choose at least one goal")


class GoalRequirement(Choice):
    """Of the goals selected in *Select your Goals*, how many must be finished?

    - **Require any one goal**: the first one you finish wins.
    - **Require all goals**: every selected goal.
    - **Require at least N goals**: the number set in *Goals Required*.
    """

    display_name = "Goal Requirements"
    rich_text_doc = True
    option_require_any_one_goal = 0
    option_require_all_goals = 1
    option_require_at_least_n_goals = 2
    default = 0


class GoalsRequired(Range):
    """How many of the selected goals must be finished.

    Does nothing unless *Goal Requirements* is *Require at least N goals*.
    It cannot be more than the number of goals selected.
    """

    display_name = "Goals Required"
    range_start = 1
    range_end = len(GOAL_NAMES)
    default = 2


class GoalZoneConquerorPercent(Range):
    """Does nothing if the *Zone Conqueror* goal is not selected.

    Finish this percentage of the quests in every zone.
    """

    display_name = "Zone Conqueror Percent"
    range_start = 1
    range_end = 100
    default = 60


class GoalQuestDexKinds(Range):
    """Does nothing if the *Quest-dex* goal is not selected.

    Finish this many different kinds of quest.
    """

    display_name = "Quest-dex Kinds"
    range_start = 1
    range_end = 80
    default = 15


class GoalMarathonKilometers(Range):
    """Does nothing if the *Marathon* goal is not selected.

    Travel this many kilometers while doing quests.
    """

    display_name = "Marathon Kilometers"
    range_start = 1
    range_end = MAX_GOAL_TARGET
    default = 42


class GoalExplorerCells(Range):
    """Does nothing if the *Explorer* goal is not selected.

    Reveal this many map cells (each about 150 meters across).
    """

    display_name = "Explorer Cells"
    range_start = 1
    range_end = MAX_GOAL_TARGET
    default = 300


class GoalStreakDays(Range):
    """Does nothing if the *Daily Habit* goal is not selected.

    Finish at least one quest on this many days in a row.
    """

    display_name = "Daily Habit Days"
    range_start = 1
    range_end = 365
    default = 7


class GoalBossRushHardQuests(Range):
    """Does nothing if the *Boss Rush* goal is not selected.

    Finish this many Hard quests.
    """

    display_name = "Boss Rush Hard Quests"
    range_start = 1
    range_end = 100
    default = 5


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
    """All options of this game, resolved per player by Archipelago."""

    goal_selection: GoalSelection
    goal_requirement: GoalRequirement
    goals_required: GoalsRequired
    goal_zone_conqueror_percent: GoalZoneConquerorPercent
    goal_quest_dex_kinds: GoalQuestDexKinds
    goal_marathon_kilometers: GoalMarathonKilometers
    goal_explorer_cells: GoalExplorerCells
    goal_streak_days: GoalStreakDays
    goal_boss_rush_hard_quests: GoalBossRushHardQuests
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


option_groups = [
    OptionGroup(
        "Goal Selection",
        [
            GoalSelection,
            GoalRequirement,
            GoalsRequired,
            GoalZoneConquerorPercent,
            GoalQuestDexKinds,
            GoalMarathonKilometers,
            GoalExplorerCells,
            GoalStreakDays,
            GoalBossRushHardQuests,
        ],
    ),
]
