package dev.apgo2

import dev.apgo2.ui.MarkerSpec
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.apgo_ffi.PositionOut

class MePinTest {
    private fun pos(
        lat: Double = 40.0,
        uncertainty: Double = 6.0,
        headingSource: String = "course",
        heading: Double? = 90.0,
        source: String = "gps",
        snap: Boolean = false,
        ageMs: Long = 500L,
        estLat: Double = lat,
    ) = PositionOut(
        lat,
        -111.0,
        estLat,
        -111.0,
        uncertainty,
        1.4,
        heading,
        heading,
        headingSource,
        false,
        0.0,
        source,
        ageMs,
        snap,
        source == "bridged",
    )

    @Test fun aMovingPinGlidesWithTheCourseArrowAndItsCircle() {
        val p = MePins.from(pos(), previous = MePins.at(40.0, -111.0, null))
        assertEquals(MarkerSpec.Me(heading = true, state = "gps"), p.spec)
        assertEquals(90f, p.bearingDeg)
        assertEquals(6f, p.accuracyM)
        assertEquals(MePins.GLIDE_MS, p.animateMs)
    }

    @Test fun noArrowWithoutAHeadingAndNoCircleUnderThreeMetres() {
        val p = MePins.from(pos(headingSource = "none", uncertainty = 2.5), previous = null)
        assertNull(p.bearingDeg)
        assertEquals(false, p.spec.heading)
        assertEquals(0f, p.accuracyM)
    }

    @Test fun aResetARelocationOrAJumpOverFiftyMetresSnaps() {
        val prev = MePins.at(40.0, -111.0, null)
        assertEquals(0L, MePins.from(pos(snap = true), prev).animateMs)
        assertEquals("111 m away", 0L, MePins.from(pos(lat = 40.001), prev).animateMs)
        assertEquals("the first pin appears in place", 0L, MePins.from(pos(), previous = null).animateMs)
    }

    // Controller note 5: `snap` is a level that stays true until the next fix; the pin jumps once per new fix, then glides.
    @Test fun aSnapJumpsOncePerNewFixNotOnEveryRefresh() {
        val jumped = MePins.from(pos(snap = true, ageMs = 500L), MePins.at(40.0, -111.0, null))
        assertEquals(0L, jumped.animateMs)
        val again = MePins.from(pos(snap = true, ageMs = 900L), jumped)
        assertEquals("the same fix, asked again later, glides", MePins.GLIDE_MS, again.animateMs)
        assertEquals(
            "a newer fix (its age dropped) that restarted the filter jumps",
            0L,
            MePins.from(pos(snap = true, ageMs = 100L), again).animateMs,
        )
        assertEquals(
            "a newer estimate (it moved) at the same age jumps",
            0L,
            MePins.from(pos(snap = true, ageMs = 900L, estLat = 40.0001), again).animateMs,
        )
        assertEquals("no snap: a newer fix glides", MePins.GLIDE_MS, MePins.from(pos(ageMs = 100L), again).animateMs)
    }

    @Test fun bridgedPinsAreHollowAndStaleOnesGrey() {
        assertEquals("bridged", MePins.from(pos(source = "bridged"), null).spec.state)
        assertEquals("stale", MePins.from(pos(source = "stale"), null).spec.state)
        assertEquals("gps", MePins.from(pos(source = "predicted"), null).spec.state)
    }

    @Test fun aRawFixPinHasItsCircleAndNoArrow() {
        val p = MePins.at(40.0, -111.0, 12.0)
        assertEquals(12f, p.accuracyM)
        assertNull(p.bearingDeg)
        assertEquals(MarkerSpec.Me(heading = false, state = "gps"), p.spec)
        assertEquals(0f, MePins.at(40.0, -111.0, 2.0).accuracyM)
    }

    // Ruling T15-glide: the glide lasts as long as the gap since the previous refresh, 200 ms to 1 s, so frequent refreshes add no lag.
    @Test fun theGlideLastsTheGapSinceThePreviousRefreshWithinBounds() {
        val prev = MePins.from(pos(), previous = MePins.at(40.0, -111.0, null), nowMs = 10_000L)
        assertEquals(10_000L, prev.atMs)
        assertEquals(300L, MePins.from(pos(ageMs = 800L), prev, nowMs = 10_300L).animateMs)
        assertEquals("never shorter than 200 ms", 200L, MePins.from(pos(ageMs = 550L), prev, nowMs = 10_050L).animateMs)
        assertEquals("never longer than 1 s", MePins.GLIDE_MS, MePins.from(pos(ageMs = 3_500L), prev, nowMs = 13_000L).animateMs)
        assertEquals("a clock that went back: the shortest glide", 200L, MePins.from(pos(), prev, nowMs = 9_000L).animateMs)
        assertEquals("a snap stays a snap", 0L, MePins.from(pos(snap = true, ageMs = 100L), prev, nowMs = 10_500L).animateMs)
    }

    @Test fun theGlideEndsItsDurationFromNowAndASnapTakesOneFrame() {
        val glide = MePins.from(pos(), previous = MePins.at(40.0, -111.0, null))
        assertEquals(11_000L, MePins.glideEndMs(glide, 10_000L))
        assertEquals(10_016L, MePins.glideEndMs(MePins.from(pos(), previous = null), 10_000L))
    }

    @Test fun everyPinImageIsListedForTheStyle() {
        assertEquals(
            6,
            MePins
                .specs()
                .map { it.key }
                .toSet()
                .size,
        )
    }
}
