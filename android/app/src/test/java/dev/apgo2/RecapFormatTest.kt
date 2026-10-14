package dev.apgo2

import dev.apgo2.ui.Units
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import uniffi.apgo_ffi.RecapChainOut
import uniffi.apgo_ffi.RecapQuestOut
import uniffi.apgo_ffi.UnitSystem

class RecapFormatTest {
    private val saved = Units.system

    @Before fun metric() {
        Units.system = UnitSystem.METRIC
    }

    @After fun restore() {
        Units.system = saved
    }

    private fun chain(
        unit: String,
        before: Double = 2140.0,
        after: Double = 6230.0,
    ) = RecapChainOut("c", "Chain", unit, 1u, before, after, listOf(1000.0, 2500.0, 5000.0, 7_500.0, 10_000.0))

    @Test fun barTextsForAStepsChain() {
        val c = chain("steps")
        assertEquals("+4,090 steps", RecapFormat.barGain(c, 6230.0))
        assertEquals("6,230 total (was 2,140)", RecapFormat.barTotal(c, 6230.0))
        assertEquals("next: 7,500", RecapFormat.barNext(c, 6230.0))
        assertEquals("All milestones reached", RecapFormat.barNext(c, 10_000.0))
    }

    @Test fun barGainForMinutesAndCells() {
        assertEquals("+42 min", RecapFormat.barGain(chain("minutes", 0.0, 42.0), 42.0))
        assertEquals("+14 new squares", RecapFormat.barGain(chain("cells", 3.0, 17.0), 17.0))
    }

    @Test fun stepsLineShowsCountedOnlyWhenDifferent() {
        assertEquals("4,812 walked · 4,090 counted", RecapFormat.stepsLine(recap(10).copy(steps = 4812, countedSteps = 4090)))
        assertNull(RecapFormat.stepsLine(recap(10).copy(steps = 4090, countedSteps = 4090)))
    }

    @Test fun subtitleHasDateDistanceAndTime() {
        val r = recap(minutes = 42).copy(distanceM = 3100.0)
        val date =
            java.text.DateFormat
                .getDateInstance(java.text.DateFormat.SHORT)
                .format(java.util.Date(r.endMs))
        assertEquals("$date · ${Units.distance(3100.0)} · 42 min", RecapFormat.subtitle(r))
    }

    @Test fun questTimeIsAShortTime() {
        val q = RecapQuestOut("Q", "P", "g", 1_000_000L, null, null)
        val expected =
            java.text.DateFormat
                .getTimeInstance(java.text.DateFormat.SHORT)
                .format(java.util.Date(1_000_000L))
        assertEquals(expected, RecapFormat.questTime(q))
    }

    @Test fun emptyResultSectionsAreLeftOut() {
        val r = recap(minutes = 10, quests = listOf("Reach the park")).copy(items = listOf("Hydrate!"), banked = 3u)
        assertEquals(
            listOf(
                "Items received" to listOf("Hydrate!"),
                "Brought home" to listOf("3 items"),
            ),
            RecapFormat.sections(r),
        )
        assertEquals(emptyList<Pair<String, List<String>>>(), RecapFormat.sections(recap(minutes = 10)))
    }

    @Test fun zonesAndChecksAreNamed() {
        val r = recap(minutes = 10).copy(zonesUnlocked = listOf(2u), checksSent = 4u)
        assertEquals(listOf("Checks sent" to listOf("4"), "Zones unlocked" to listOf("Zone 2")), RecapFormat.sections(r))
    }
}
