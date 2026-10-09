package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ApPollTest {
    @Test fun pollsFastRightAfterActivityAndSlowsDownWhileQuiet() {
        val p = ApPoll()
        assertEquals("activity: fast", ApPoll.FAST_MS, p.next(active = true))
        val quiet = (1..20).map { p.next(active = false) }
        assertTrue("never faster while quiet", quiet.zipWithNext().all { (a, b) -> b >= a })
        assertEquals("settles at the slow rate", ApPoll.SLOW_MS, quiet.last())
        assertEquals("activity again: fast at once", ApPoll.FAST_MS, p.next(active = true))
    }

    @Test fun theGameSyncsOnlyWhenTheServerSentSomethingOrNeverSynced() {
        assertTrue("first time", ApPoll.needsSync(serverChanged = false, synced = false))
        assertTrue("items or data arrived", ApPoll.needsSync(serverChanged = true, synced = true))
        assertFalse("nothing new", ApPoll.needsSync(serverChanged = false, synced = true))
    }
}
