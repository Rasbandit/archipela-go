package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

// apworld/ap_go2/constants.py, its path passed in by build.gradle.kts, so a rename there fails these tests.
private fun apworldConstants(): String {
    val path = System.getProperty("apworld.constants") ?: error("system property apworld.constants is not set (build.gradle.kts)")
    val file = File(path)
    check(file.isFile) { "apworld constants not found at $path" }
    return file.readText()
}

// The body of the top-level `NAME... = { ... }` dict literal in [source].
private fun pyDict(
    source: String,
    name: String,
): String =
    Regex("""^$name\b[^=\n]*=\s*\{(.*?)^\}""", setOf(RegexOption.MULTILINE, RegexOption.DOT_MATCHES_ALL))
        .find(source)
        ?.groupValues
        ?.get(1)
        ?: error("$name dict not found in apworld constants.py")

class HelpTextTest {
    private val constants = apworldConstants()

    // The apworld's goal ids and player-facing names (GOAL_NAMES).
    private val goalNames =
        Regex(""""([^"]+)"\s*:\s*"([^"]+)"""")
            .findAll(pyDict(constants, "GOAL_NAMES"))
            .associate { it.groupValues[1] to it.groupValues[2] }
            .also { check(it.isNotEmpty()) { "GOAL_NAMES has no entries" } }

    // The player-selectable quest families (the keys of FAMILY_MODES).
    private val families =
        Regex("""^\s*"([^"]+)"\s*:""", RegexOption.MULTILINE)
            .findAll(pyDict(constants, "FAMILY_MODES"))
            .map { it.groupValues[1] }
            .toSet()
            .also { check(it.isNotEmpty()) { "FAMILY_MODES has no entries" } }

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
