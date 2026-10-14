package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class ZoneModesReadTest {
    @Test fun aPlayableSeedGivesItsModesAndNoReason() {
        assertEquals(listOf("walk", "run") to null, readZoneModes("{}") { listOf("walk", "run") })
    }

    @Test fun anOlderSeedShowsTheCoresReason() {
        val why = "unsupported slot_data schema 3 (this app understands [4])"
        assertEquals(emptyList<String>() to why, readZoneModes("{}") { throw uniffi.apgo_ffi.CoreException.Failed(why) })
    }

    @Test fun noSlotDataYetIsNoReason() {
        assertEquals(emptyList<String>() to null, readZoneModes(null) { error("not called") })
    }
}
