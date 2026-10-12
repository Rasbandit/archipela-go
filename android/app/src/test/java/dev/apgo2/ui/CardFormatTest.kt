package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.apgo_ffi.PlaceOut

class CardFormatTest {
    @Test fun aTitledPlaceIsQuotedAndFollowedByItsKind() {
        assertEquals(CardFormat.Place("“Allow Me”", " · artwork"), CardFormat.place(PlaceOut("Allow Me", "Artwork", quoted = true)))
    }

    @Test fun aNamedPlaceIsNotQuotedUnlessItIsATitle() {
        assertEquals(CardFormat.Place("Liberty Park", " · park"), CardFormat.place(PlaceOut("Liberty Park", "Park", quoted = false)))
    }

    @Test fun anUnnamedPlaceShowsWhatItIsAlone() {
        assertEquals(CardFormat.Place(null, "Bench"), CardFormat.place(PlaceOut(null, "Bench", quoted = false)))
    }

    @Test fun aPlaceWithNoKindShowsItsNameAlone() {
        assertEquals(CardFormat.Place("Bonneville Trail", null), CardFormat.place(PlaceOut("Bonneville Trail", null, quoted = false)))
    }

    @Test fun effortIsRoundedMinutes() {
        assertEquals("~9 min", CardFormat.effort(9.4))
        assertEquals("~10 min", CardFormat.effort(9.6))
        assertEquals("~1 min", CardFormat.effort(0.2))
    }
}
