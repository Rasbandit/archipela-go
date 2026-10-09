package dev.apgo2.presence

import org.junit.Assert.assertEquals
import org.junit.Test

class PresenceSeedingTest {
    private fun started(t: Long = 0) = PresenceSeeding(3_000).apply { restart(t) }

    @Test fun eachSignalSeedsWhenItsOwnReadingArrives() {
        val s = started()
        assertEquals("nothing yet", SeedPlan(false, false), s.poll(50, wifiReported = false, bluetoothReady = false))
        assertEquals("wifi at 100", SeedPlan(true, false), s.poll(100, wifiReported = true, bluetoothReady = false))
        assertEquals("bluetooth at 200", SeedPlan(false, true), s.poll(200, wifiReported = true, bluetoothReady = true))
        assertEquals("complete", true, s.complete)
        assertEquals("no longer waiting", false, s.waiting)
    }

    @Test fun neverOnWifiSeedsHomeExactlyAtTheTimeout() {
        val s = started(1_000)
        assertEquals("just before", SeedPlan(false, false), s.poll(3_999, wifiReported = false, bluetoothReady = false))
        assertEquals("at timeout", SeedPlan(true, true), s.poll(4_000, wifiReported = false, bluetoothReady = false))
    }

    @Test fun bluetoothThatNeverAnswersSeedsTheCarAtTheTimeout() {
        val s = started()
        s.poll(100, wifiReported = true, bluetoothReady = false)
        assertEquals("before", SeedPlan(false, false), s.poll(2_999, wifiReported = true, bluetoothReady = false))
        assertEquals("at timeout", SeedPlan(false, true), s.poll(3_000, wifiReported = true, bluetoothReady = false))
    }

    @Test fun aSignalIsNeverSeededTwice() {
        val s = started()
        s.poll(100, wifiReported = true, bluetoothReady = true)
        assertEquals(SeedPlan(false, false), s.poll(5_000, wifiReported = true, bluetoothReady = true))
    }

    @Test fun restartResetsEverything() {
        val s = started()
        s.poll(100, wifiReported = true, bluetoothReady = true)
        s.restart(10_000)
        assertEquals("waiting again", true, s.waiting)
        assertEquals("not complete", false, s.complete)
        assertEquals(
            "timeout counts from the restart",
            SeedPlan(false, false),
            s.poll(12_999, wifiReported = false, bluetoothReady = false),
        )
        assertEquals(SeedPlan(true, true), s.poll(13_000, wifiReported = false, bluetoothReady = false))
    }

    @Test fun waitingFromTheStartUntilTheFirstReadingsAreIn() {
        // Before the Wi-Fi watcher has started, home is unknown: deciding then turned GPS on for a moment at every app start.
        assertEquals("never started: still waiting", true, PresenceSeeding().waiting)
        assertEquals("started and incomplete", true, started().waiting)
    }
}
