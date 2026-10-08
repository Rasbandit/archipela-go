package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class HelpTextTest {
    // The apworld's goal ids and player-facing names (apworld/ap_go2/constants.py GOAL_NAMES).
    private val goalNames =
        mapOf(
            "macguffin_short" to "Letter Hunt",
            "macguffin_long" to "Letter Hunt XL",
            "all_trips" to "Completionist",
            "boss" to "The Big One",
            "treasure_hunt" to "Treasure Hunt",
            "zone_conqueror" to "Zone Conqueror",
            "well_rounded" to "Well Rounded",
            "quest_dex" to "Quest-dex",
            "marathon" to "Marathon",
            "explorer" to "Explorer",
            "streak" to "Daily Habit",
            "boss_rush" to "Boss Rush",
        )

    // The player-selectable quest families (apworld/ap_go2/constants.py FAMILY_MODES).
    private val families = setOf("reach", "dwell", "landmark", "courier", "away", "explore", "trail", "water", "park", "steps")

    private val topics =
        with(Help) {
            listOf(
                zones,
                travelMode,
                questTypes,
                goals,
                goalRule,
                goalTarget,
                questCount,
                difficulty,
                minutesPerTier,
                fog,
                traps,
                bonus,
                terrain,
                stairs,
                awayZone,
                awayDistance,
                archipelago,
            ) + families.values + goalDescriptions.values
        }

    @Test fun everyGoalIsExplainedUnderTheNameTheApworldUses() {
        assertEquals(goalNames, Help.goalDescriptions.mapValues { it.value.title })
    }

    @Test fun everyQuestFamilyIsExplained() {
        assertEquals(families, Help.families.keys)
    }

    @Test fun everyTopicHasATitleAndAWholeSentence() {
        topics.forEach {
            assertTrue("blank title: $it", it.title.isNotBlank())
            assertTrue("body should end a sentence: ${it.title}", it.body.trimEnd().endsWith("."))
            assertTrue("double space in ${it.title}", "  " !in it.body)
        }
    }

    @Test fun setupCopyHasNoBrokenJoins() {
        listOf(
            SetupText.HOME_BASE_CARD,
            SetupText.HOME_BASE_UNSET,
            SetupText.WIFI_WHY,
            SetupText.CAR_WHY,
            SetupText.WIFI_NONE_FOUND,
            SetupText.WIFI_NEEDS_LOCATION,
            SetupText.CAR_NEEDS_BLUETOOTH,
            SetupText.CAR_NONE_PAIRED,
        ).forEach {
            assertTrue(it, it.endsWith("."))
            assertTrue(it, "  " !in it)
        }
    }
}
