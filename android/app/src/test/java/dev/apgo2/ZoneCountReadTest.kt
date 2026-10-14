package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class ZoneCountReadTest {
    @Test fun aPlayableSeedGivesItsZoneCountAndNoReason() {
        assertEquals(3 to null, readZoneCount("{}") { 3u })
    }

    @Test fun anOlderSeedShowsTheCoresReason() {
        val why = "unsupported slot_data schema 4 (this app understands [5])"
        assertEquals(0 to why, readZoneCount("{}") { throw uniffi.apgo_ffi.CoreException.Failed(why) })
    }

    @Test fun noSlotDataYetIsNoReason() {
        assertEquals(0 to null, readZoneCount(null) { error("not called") })
    }
}
