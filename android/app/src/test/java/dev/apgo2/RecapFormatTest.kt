package dev.apgo2

import dev.apgo2.ui.Units
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import uniffi.apgo_ffi.UnitSystem

class RecapFormatTest {
    private val saved = Units.system

    @Before fun metric() {
        Units.system = UnitSystem.METRIC
    }

    @After fun restore() {
        Units.system = saved
    }

    @Test fun statsShowTimeDistanceStepsAndSquares() {
        val r = recap(minutes = 75).copy(steps = 6543, distanceM = 4200.0, newCells = 12u)
        assertEquals(
            listOf("Time out" to "1 h 15 min", "Distance" to Units.distance(4200.0), "Steps" to "6,543", "New squares" to "12"),
            RecapFormat.stats(r),
        )
    }

    @Test fun emptyResultSectionsAreLeftOut() {
        val r = recap(minutes = 10, quests = listOf("Reach the park")).copy(items = listOf("Hydrate!"), banked = 3u)
        assertEquals(
            listOf(
                "Quests completed" to listOf("Reach the park"),
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
