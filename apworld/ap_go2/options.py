from dataclasses import dataclass

from Options import (  # type: ignore[import-not-found]
    Choice,
    DeathLink,
    NamedRange,
    OptionSet,
    PerGameCommonOptions,
    Range,
    Toggle,
)

from ap_go2.constants import MODES


class Goal(Choice):
    """Win condition: check every trip, or collect the letters APGO (short) or ARCHIPELAGO."""

    display_name = "Goal"
    option_all_trips = 0
    option_macguffin_short = 1
    option_macguffin_long = 2
    default = 1


class NumberOfTrips(Range):
    """How many trips (locations) exist in your world."""

    display_name = "Number of Trips"
    range_start = 1
    range_end = 1000
    default = 100


class MinimumDistance(NamedRange):
    """Closest a trip may be from home, in meters."""

    display_name = "Minimum Distance"
    range_start = 100
    range_end = 5000
    default = 500
    special_range_names = {"1k": 1000, "2k": 2000}


class MaximumDistance(NamedRange):
    """Farthest a trip may be from home, in meters."""

    display_name = "Maximum Distance"
    range_start = 1000
    range_end = 100000
    default = 5000
    special_range_names = {
        "2k": 2000,
        "5k": 5000,
        "10k": 10000,
        "half_marathon": 21097,
        "marathon": 42195,
        "50k": 50000,
        "100k": 100000,
    }


class NumberOfLocks(Range):
    """Progressive Keys in the pool; each unlocks the next area of trips."""

    display_name = "Number of Locks"
    range_start = 0
    range_end = 10
    default = 3


class TrapRate(Range):
    """Percent of free item slots that become traps."""

    display_name = "Trap Rate"
    range_start = 0
    range_end = 100
    default = 50


class EnableDistanceReductions(Toggle):
    """Far trips need Distance Reduction items before they are in logic."""

    display_name = "Enable Distance Reductions"


class EnableScoutingDistanceBonuses(Toggle):
    """Add Scouting Distance items to the pool."""

    display_name = "Enable Scouting Distance Bonuses"


class EnableCollectionDistanceBonuses(Toggle):
    """Add Collection Distance items to the pool."""

    display_name = "Enable Collection Distance Bonuses"


class AllowedModes(OptionSet):
    """Travel modes you are willing to use; trips are assigned one of these."""

    display_name = "Allowed Modes"
    valid_keys = frozenset(MODES)
    default = frozenset({"walk"})


class ReturnHome(Toggle):
    """Client requires returning home between trips."""

    display_name = "Return Home"


class ReductionPercent(Range):
    """Percent each Distance Reduction shrinks effective trip distance."""

    display_name = "Reduction Percent"
    range_start = 1
    range_end = 25
    default = 8


@dataclass
class ApGo2Options(PerGameCommonOptions):
    goal: Goal
    number_of_trips: NumberOfTrips
    minimum_distance: MinimumDistance
    maximum_distance: MaximumDistance
    number_of_locks: NumberOfLocks
    trap_rate: TrapRate
    enable_distance_reductions: EnableDistanceReductions
    enable_scouting_distance_bonuses: EnableScoutingDistanceBonuses
    enable_collection_distance_bonuses: EnableCollectionDistanceBonuses
    allowed_modes: AllowedModes
    return_home: ReturnHome
    reduction_percent: ReductionPercent
    death_link: DeathLink
