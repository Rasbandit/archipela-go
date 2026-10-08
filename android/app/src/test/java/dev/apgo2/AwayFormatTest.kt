package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class AwayFormatTest {
    @Test fun durationsReadNaturally() {
        assertEquals("under a minute", AwayFormat.duration(30_000))
        assertEquals("5 min", AwayFormat.duration(5 * 60_000))
        assertEquals("1 h", AwayFormat.duration(60 * 60_000))
        assertEquals("3 h 12 min", AwayFormat.duration((3 * 60 + 12) * 60_000L))
        assertEquals("under a minute", AwayFormat.duration(-5))
    }

    @Test fun distanceSwitchesToKilometres() {
        assertEquals("850 m", AwayFormat.distance(850.4))
        assertEquals("1.2 km", AwayFormat.distance(1234.0))
        assertEquals("0 m", AwayFormat.distance(0.0))
    }

    @Test fun kindsHaveReadableLabelsAndUnknownFallsBack() {
        assertEquals("Quests completed", AwayFormat.kindLabel("quest_done"))
        assertEquals("Bad GPS signal", AwayFormat.kindLabel("fix_rejected"))
        assertEquals("something_new", AwayFormat.kindLabel("something_new"))
    }

    @Test fun countsLeaveOutAppStateNoise() {
        val shown = AwayFormat.visibleKinds(listOf("quest_done", "app_background", "app_foreground", "trap"))
        assertEquals(listOf("quest_done", "trap"), shown)
    }
}

class ActivityFormatTest {
    @Test fun storyEventsAlwaysShowAndTechnicalOnesOnlyOnRequest() {
        listOf("quest_done", "reward", "trap", "zone_unlocked", "goal", "info", "item_received", "check_sent").forEach {
            assertEquals(it, true, ActivityFormat.shown(it, details = false))
        }
        listOf("near_miss", "fix_rejected", "app_foreground", "app_background", "discovered").forEach {
            assertEquals(it, false, ActivityFormat.shown(it, details = false))
            assertEquals(it, true, ActivityFormat.shown(it, details = true))
        }
    }

    @Test fun technicalKindsAreLabelled() {
        assertEquals("Near a quest", AwayFormat.kindLabel("near_miss"))
        assertEquals("App left the screen", AwayFormat.kindLabel("app_background"))
        assertEquals("App opened", AwayFormat.kindLabel("app_foreground"))
    }
}
