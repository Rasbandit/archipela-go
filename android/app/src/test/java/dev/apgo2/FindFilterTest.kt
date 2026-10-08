package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.apgo_ffi.FindOut
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.KindOut

class FindFilterTest {
    private val finds =
        listOf(
            find("Oak Bench", "", "Bench Warmer"),
            find("Library", FindFilter.FAVORITE, "Book Worm"),
            find("Car Park", FindFilter.BANNED),
        )

    private fun find(
        name: String,
        mark: String = "",
        vararg kinds: String,
    ) = FindOut(
        name,
        name,
        true,
        kinds.map { KindOut(it, it, "rest", "", "") },
        emptyList(),
        "bench",
        "rest",
        GeoPoint(1.0, 2.0),
        0.0,
        mark,
    )

    private fun shown(
        filter: String,
        query: String,
    ) = finds.filter { FindFilter.matches(it, filter, query) }.map { it.name }

    @Test fun allWithNoQueryShowsEverything() = assertEquals(listOf("Oak Bench", "Library", "Car Park"), shown(FindFilter.ALL, ""))

    @Test fun aMarkFilterShowsOnlyFindsWithThatMark() {
        assertEquals(listOf("Library"), shown(FindFilter.FAVORITE, ""))
        assertEquals(listOf("Car Park"), shown(FindFilter.BANNED, ""))
    }

    @Test fun theQueryMatchesTheNameIgnoringCase() = assertEquals(listOf("Oak Bench"), shown(FindFilter.ALL, "oak"))

    @Test fun theQueryAlsoMatchesAQuestKindName() = assertEquals(listOf("Library"), shown(FindFilter.ALL, "WORM"))

    // Keyboard autocomplete adds a trailing space.
    @Test fun spacesAroundTheQueryAreIgnored() = assertEquals(listOf("Oak Bench"), shown(FindFilter.ALL, " bench "))

    @Test fun aBlankQueryCountsAsNoQuery() = assertEquals(3, shown(FindFilter.ALL, "   ").size)

    @Test fun filterAndQueryMustBothMatch() {
        assertTrue(FindFilter.matches(finds[1], FindFilter.FAVORITE, "lib"))
        assertFalse(FindFilter.matches(finds[1], FindFilter.BANNED, "lib"))
        assertFalse(FindFilter.matches(finds[1], FindFilter.FAVORITE, "oak"))
    }
}
