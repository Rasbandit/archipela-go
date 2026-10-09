package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class ServiceStartTest {
    @Test fun aStartFromTheAppNeedsNothingMore() {
        // The activity is up: its effects start presence, steps and GPS.
        assertEquals(ServiceStart.FromApp, ServiceStart.decide(restarted = false, playing = true))
    }

    @Test fun aRestartWithAGameStartsTrackingWithoutTheScreen() {
        assertEquals(ServiceStart.ResumeHeadless, ServiceStart.decide(restarted = true, playing = true))
    }

    @Test fun aRestartWithNoGameStopsTheService() {
        // Nothing to track: no lingering notification.
        assertEquals(ServiceStart.Stop, ServiceStart.decide(restarted = true, playing = false))
    }
}
