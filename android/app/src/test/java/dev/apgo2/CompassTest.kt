package dev.apgo2

import android.hardware.SensorManager
import android.view.Surface
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.apgo_ffi.HeadingIn

class CompassTest {
    @Test fun theAxesFollowTheScreenRotation() {
        assertEquals(SensorManager.AXIS_X to SensorManager.AXIS_Y, CompassAxes.forRotation(Surface.ROTATION_0))
        assertEquals(SensorManager.AXIS_Y to SensorManager.AXIS_MINUS_X, CompassAxes.forRotation(Surface.ROTATION_90))
        assertEquals(SensorManager.AXIS_MINUS_X to SensorManager.AXIS_MINUS_Y, CompassAxes.forRotation(Surface.ROTATION_180))
        assertEquals(SensorManager.AXIS_MINUS_Y to SensorManager.AXIS_X, CompassAxes.forRotation(Surface.ROTATION_270))
        assertEquals("an unknown rotation is upright", SensorManager.AXIS_X to SensorManager.AXIS_Y, CompassAxes.forRotation(7))
    }

    @Test fun eachReadingCarriesItsOwnAccuracyAndAnUnusableOneFallsBackToTheLastChange() {
        val high = SensorManager.SENSOR_STATUS_ACCURACY_HIGH
        val low = SensorManager.SENSOR_STATUS_ACCURACY_LOW
        val unreliable = SensorManager.SENSOR_STATUS_UNRELIABLE
        assertEquals("the event's own value wins", high, CompassAccuracies.effective(eventAccuracy = high, lastChanged = unreliable))
        assertEquals(low, CompassAccuracies.effective(eventAccuracy = low, lastChanged = high))
        assertEquals(
            "an unreliable event: the last change",
            high,
            CompassAccuracies.effective(eventAccuracy = unreliable, lastChanged = high),
        )
        assertEquals("no contact (-1): the last change", low, CompassAccuracies.effective(eventAccuracy = -1, lastChanged = low))
        assertEquals("nothing better known", unreliable, CompassAccuracies.effective(eventAccuracy = unreliable, lastChanged = unreliable))
    }

    @Test fun aFusedHeadingErrorNamesItsAccuracyBand() {
        // The error is half a 95 % cone (about two sigma); the bands are the core's sigmas 15 / 30 / 45 degrees.
        assertEquals("high", FusedCompass.accuracyName(30.0))
        assertEquals("medium", FusedCompass.accuracyName(31.0))
        assertEquals("medium", FusedCompass.accuracyName(60.0))
        assertEquals("low", FusedCompass.accuracyName(90.0))
        assertEquals("unreliable", FusedCompass.accuracyName(91.0))
        assertEquals("no idea", "unreliable", FusedCompass.accuracyName(180.0))
        assertEquals("unreliable", FusedCompass.accuracyName(Double.NaN))
        assertEquals("unreliable", FusedCompass.accuracyName(-1.0))
    }

    @Test fun aFusedSampleIsATrueNorthReadingWithItsOwnError() {
        val r = FusedCompass.reading(headingDeg = 350f, errorDeg = 12f, pitchDeg = 5.0, rollDeg = -3.0, eventMs = 1_700L)
        assertEquals(
            CompassReading(350.0, trueNorth = true, accuracy = "high", errorDeg = 12.0, pitchDeg = 5.0, rollDeg = -3.0, eventMs = 1_700L),
            r,
        )
    }

    @Test fun onlyAMagneticReadingIsTurnedToTrueNorth() {
        val magnetic =
            CompassReading(350.0, trueNorth = false, accuracy = "medium", errorDeg = null, pitchDeg = 1.0, rollDeg = 2.0, eventMs = 9L)
        assertEquals(
            HeadingIn(azimuthDeg = 5.0, accuracy = "medium", pitchDeg = 1.0, rollDeg = 2.0, tMs = 9L, errorDeg = null),
            CompassReadings.headingIn(magnetic, declinationDeg = 15.0),
        )
        val fused = magnetic.copy(trueNorth = true, accuracy = "high", errorDeg = 12.0)
        val h = CompassReadings.headingIn(fused, declinationDeg = 15.0)
        assertEquals("already true north", 350.0, h.azimuthDeg, 0.0)
        assertEquals(12.0, h.errorDeg!!, 0.0)
        assertNull(CompassReadings.headingIn(magnetic, 15.0).errorDeg)
    }

    @Test fun theFusedCompassIsUsedOnlyOnScreenWithPlayServices() {
        // It only delivers while the app is in the foreground; the rotation vector keeps the pocket compass going with the screen off.
        assertEquals(true, CompassSource.fused(gms = true, onScreen = true))
        assertEquals(false, CompassSource.fused(gms = true, onScreen = false))
        assertEquals(false, CompassSource.fused(gms = false, onScreen = true))
    }

    @Test fun theCompassRunsAtFiveHertzOnScreenAndOnceASecondOtherwise() {
        assertEquals(200_000, CompassRate.periodUs(screenOn = true, appVisible = true))
        assertEquals(1_000_000, CompassRate.periodUs(screenOn = false, appVisible = true))
        assertEquals(1_000_000, CompassRate.periodUs(screenOn = true, appVisible = false))
    }
}

class FusedHeadingWatchTest {
    private fun started(): FusedHeadingWatch =
        FusedHeadingWatch().apply {
            wanted(gms = true, onScreen = true)
            started(0L)
        }

    @Test fun theFusedCompassIsTriedOnlyWithPlayServicesOnScreen() {
        val w = FusedHeadingWatch()
        assertTrue(w.wanted(gms = true, onScreen = true))
        assertFalse(w.wanted(gms = true, onScreen = false))
        assertFalse(w.wanted(gms = false, onScreen = true))
    }

    @Test fun aFailureIsRememberedUntilTheVisibilityChanges() {
        val w = FusedHeadingWatch()
        assertTrue(w.wanted(gms = true, onScreen = true))
        w.failed()
        assertFalse("no retry while nothing changed", w.wanted(gms = true, onScreen = true))
        assertFalse(w.wanted(gms = true, onScreen = false))
        assertTrue("back on screen: try again", w.wanted(gms = true, onScreen = true))
    }

    @Test fun silenceForThreeSecondsFallsBack() {
        val w = started()
        assertNull(w.fallBack(2_999L))
        assertEquals("silent", w.fallBack(3_000L))
        assertFalse("given up until the visibility changes", w.wanted(gms = true, onScreen = true))
    }

    @Test fun readingsKeepItAndAGapAfterThemCountsFromTheLastOne() {
        val w = started()
        w.reading(2_000L, 10.0)
        assertNull(w.fallBack(4_999L))
        assertEquals("silent", w.fallBack(5_000L))
    }

    @Test fun noHeadingForFiveSecondsFallsBack() {
        val w = started()
        w.reading(1_000L, 180.0)
        w.reading(5_000L, 180.0)
        assertNull("4 s of no heading is still waited out", w.fallBack(5_500L))
        w.reading(6_000L, 200.0)
        assertEquals("no_heading", w.fallBack(6_000L))
    }

    @Test fun aGoodReadingRestartsTheNoHeadingClock() {
        val w = started()
        w.reading(1_000L, 180.0)
        w.reading(4_000L, 25.0)
        w.reading(5_000L, 180.0)
        w.reading(9_000L, Double.NaN)
        assertNull(w.fallBack(9_500L))
        assertEquals("no_heading", w.fallBack(10_000L))
    }

    @Test fun aRestartClearsTheClocks() {
        val w = started()
        w.reading(1_000L, 180.0)
        w.started(5_000L)
        assertNull(w.fallBack(7_000L))
    }
}

class HeadingCompareTest {
    private fun field(
        fields: List<Pair<String, Any?>>,
        key: String,
    ): Double = fields.toMap()[key] as Double

    @Test fun aTrueNorthFusedHeadingMatchesTheCorrectedRotationVector() {
        // 10 deg east declination: magnetic 80 is true 90; a true-north fused 90 differs by 0 from true and 10 from magnetic.
        val f = HeadingCompare.fields(fusedAzDeg = 90.0, rotationMagAzDeg = 80.0, declinationDeg = 10.0)
        assertEquals(90.0, field(f, "rv_true_az"), 1e-9)
        assertEquals(0.0, field(f, "diff_true"), 1e-9)
        assertEquals(10.0, field(f, "diff_mag"), 1e-9)
    }

    @Test fun onlyAnUprightScreenIsCompared() {
        assertTrue(HeadingCompare.upright(Surface.ROTATION_0))
        for (r in listOf(
            Surface.ROTATION_90,
            Surface.ROTATION_180,
            Surface.ROTATION_270,
        )) {
            assertFalse("rotation $r", HeadingCompare.upright(r))
        }
    }

    @Test fun theFirstRotationEventIsSkippedUnlessItCameLate() {
        assertFalse("first event, at once", HeadingCompare.settled(eventNumber = 1, sinceRegisterMs = 50L))
        assertTrue("second event", HeadingCompare.settled(eventNumber = 2, sinceRegisterMs = 60L))
        assertTrue("first event, 200 ms on", HeadingCompare.settled(eventNumber = 1, sinceRegisterMs = 200L))
    }

    @Test fun differencesAreSignedTheShortWayAcrossNorth() {
        val f = HeadingCompare.fields(fusedAzDeg = 355.0, rotationMagAzDeg = -10.0, declinationDeg = 12.0)
        assertEquals(2.0, field(f, "rv_true_az"), 1e-9)
        assertEquals(-7.0, field(f, "diff_true"), 1e-9)
        assertEquals(5.0, field(f, "diff_mag"), 1e-9)
    }
}
