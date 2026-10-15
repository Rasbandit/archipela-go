package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.apgo_ffi.ApServerOut

class LanHoldTest {
    private val lan = ApServerOut("192.168.1.2:38281", "Tester")

    private fun held() = LanHold().apply { hold(lan) }

    @Test fun nothingHeldReleasesNothing() {
        assertNull(LanHold().release(lacks = false, gameOpen = true))
    }

    @Test fun accessGrantedWithTheGameOpenReleasesTheServerOnce() {
        val h = held()
        assertEquals(lan, h.release(lacks = false, gameOpen = true))
        assertNull("released: no longer held", h.server)
        assertNull("a second release connects nothing", h.release(lacks = false, gameOpen = true))
    }

    @Test fun stillWithoutAccessKeepsWaiting() {
        val h = held()
        assertNull(h.release(lacks = true, gameOpen = true))
        assertEquals("the prompt stays", lan, h.server)
    }

    @Test fun aHoldWhoseGameIsNoLongerOpenIsDroppedNotConnected() {
        val h = held()
        assertNull("paused or closed: never connect its server", h.release(lacks = false, gameOpen = false))
        assertNull("and the prompt goes", h.server)
    }

    @Test fun dropForgetsTheServer() {
        val h = held()
        h.drop()
        assertNull(h.release(lacks = false, gameOpen = true))
    }
}
