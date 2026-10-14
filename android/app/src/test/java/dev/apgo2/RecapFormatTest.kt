package dev.apgo2

import dev.apgo2.ui.Units
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import uniffi.apgo_ffi.RecapChainOut
import uniffi.apgo_ffi.RecapForagerOut
import uniffi.apgo_ffi.RecapQuestOut
import uniffi.apgo_ffi.UnitSystem
import java.text.SimpleDateFormat
import java.util.Locale
import java.util.TimeZone

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

    private fun utc(pattern: String) = SimpleDateFormat(pattern, Locale.US).apply { timeZone = TimeZone.getTimeZone("UTC") }

    @Test fun subtitleHasDayTimesDistanceAndDuration() {
        val start = 1_760_461_920_000L // Tue 2025-10-14 17:12 UTC
        val r = recap(minutes = 0).copy(startMs = start, endMs = start + 42 * 60_000L, distanceM = 3100.0)
        assertEquals(
            "Tue 10/14 · 5:12 PM – 5:54 PM · 3.1 km · 42 min",
            RecapFormat.subtitle(r, time = utc("h:mm a"), day = utc("EEE M/d")),
        )
    }

    @Test fun subtitleShowsTheStartDayWhenTheWalkCrossesMidnight() {
        val start = 1_760_485_200_000L // Tue 2025-10-14 23:40 UTC
        val r = recap(minutes = 0).copy(startMs = start, endMs = start + 40 * 60_000L, distanceM = 3100.0)
        assertEquals(
            "Tue 10/14 · 11:40 PM – 12:20 AM · 3.1 km · 40 min",
            RecapFormat.subtitle(r, time = utc("h:mm a"), day = utc("EEE M/d")),
        )
    }

    private fun forager(
        theme: String = "pinecones",
        need: UInt = 7u,
        carried: UInt = 0u,
    ) = RecapForagerOut("Pine hunt", theme, need, 5u, 7u, 2u, carried)

    @Test fun foragerLineCapitalisesThemeAndRoundsBanked() {
        assertEquals("Pinecones · +2 banked · 7 / 7", RecapFormat.foragerLine(forager(), 7.0))
        assertEquals("Pinecones · +1 banked · 6 / 7", RecapFormat.foragerLine(forager(), 6.4))
    }

    @Test fun bagLineIsNullWhenEmpty() {
        assertNull(RecapFormat.bagLine(forager()))
        assertEquals("1 still in your bag", RecapFormat.bagLine(forager(carried = 1u)))
        assertEquals("2 still in your bag", RecapFormat.bagLine(forager(carried = 2u)))
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
