package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class RawTrackTest {
    private fun sample(
        tMs: Long = 1_000L,
        elapsedNs: Long = 5_000_000_000L,
    ) = FixSample(
        tMs = tMs,
        elapsedNs = elapsedNs,
        lat = 40.5,
        lon = -111.9,
        accuracyM = 4.5f,
        speedMps = 1.25f,
        speedAccMps = 0.5f,
        bearingDeg = 90f,
        bearingAccDeg = 12f,
        altitudeM = 1400.0,
        verticalAccM = 3f,
        provider = "fused",
        mock = false,
    )

    @Test fun aSampleBecomesTheCoreFixAndAMockIsKeptUnlessAllowed() {
        val f = sample().copy(mock = true).toFixIn()
        assertEquals(1_000L, f.tMs)
        assertEquals(4.5, f.accuracyM, 1e-6)
        assertEquals(1.25, f.speedMps!!, 1e-6)
        assertEquals("fused", f.provider)
        assertEquals(true, f.mock)
        assertEquals(false, sample().copy(mock = true).toFixIn(mockAllowed = true).mock)
    }

    @Test fun aSampleWithoutAccuracyIsUnusablyCoarse() {
        assertEquals(1_000.0, sample().copy(accuracyM = null).toFixIn().accuracyM, 0.0)
    }

    @Test fun fixTimeComesFromTheMonotonicClockWhenThePhoneHasIt() {
        // Fix taken 2 s before now on the elapsed clock: its wall time is now minus 2 s, whatever Location.time says.
        assertEquals(
            98_000L,
            FixTime.wallMs(fixTimeMs = 50_000L, fixElapsedNs = 8_000_000_000L, nowWallMs = 100_000L, nowElapsedNs = 10_000_000_000L),
        )
    }

    @Test fun withoutAnElapsedTimeTheFixTimeIsUsedAsIs() {
        assertEquals(
            50_000L,
            FixTime.wallMs(fixTimeMs = 50_000L, fixElapsedNs = 0L, nowWallMs = 100_000L, nowElapsedNs = 10_000_000_000L),
        )
    }

    @Test fun aLastKnownFixCountsAsFreshForTwoMinutesOnly() {
        val now = 1_000_000_000_000L
        val min = 60_000_000_000L
        assertEquals("1 min old", true, FixTime.fresh(now - min, now))
        assertEquals("exactly 2 min", true, FixTime.fresh(now - 2 * min, now))
        assertEquals("3 min old", false, FixTime.fresh(now - 3 * min, now))
        assertEquals("no elapsed time: unknown age", false, FixTime.fresh(0L, now))
    }

    @Test fun aBatchIsDeliveredOldestFirst() {
        // Review Focus 1: screen-off batches arrive newest first on some phones.
        val batch = listOf(sample(elapsedNs = 3L), sample(elapsedNs = 1L), sample(elapsedNs = 2L))
        assertEquals(listOf(1L, 2L, 3L), FixTime.oldestFirst(batch) { it.elapsedNs }.map { it.elapsedNs })
    }

    @Test fun aRawFixLineHasEveryFieldInTheBenchFormat() {
        val f = RawLines.fix(sample())
        assertEquals(
            listOf("tf", "ert", "lat", "lon", "acc", "spd", "spd_acc", "brg", "brg_acc", "alt", "valt", "prov", "mock"),
            f.keys.toList(),
        )
        assertEquals(1_000L, f["tf"])
        assertEquals("fused", f["prov"])
    }

    @Test fun missingFieldsAreNullNotZero() {
        val f = RawLines.fix(sample().copy(speedMps = null, bearingDeg = null, accuracyM = null))
        assertEquals(null, f["spd"])
        assertEquals(null, f["brg"])
        assertEquals(null, f["acc"])
    }

    @Test fun stepHeadingAndStateLinesCarryTheirFields() {
        assertEquals(mapOf("total" to 12L, "te" to 34L), RawLines.steps(12L, 34L))
        assertEquals(
            mapOf("te" to 1_700L, "az" to 270.0, "acc" to "high", "pitch" to 5.0, "roll" to -3.0),
            RawLines.heading(270.0, "high", 5.0, -3.0, 1_700L),
        )
        assertEquals(
            "a fused heading also records its own error",
            mapOf("te" to 1_700L, "az" to 270.0, "acc" to "high", "pitch" to 5.0, "roll" to -3.0, "err" to 12.0),
            RawLines.heading(270.0, "high", 5.0, -3.0, 1_700L, errorDeg = 12.0),
        )
        assertEquals(
            mapOf("presence" to "InZone", "counting" to true, "zone" to "inside", "app_visible" to false),
            RawLines.state("InZone", true, "inside", false),
        )
    }

    @Test fun mockFixesAreOnlyAllowedInADebugBuildWithTheFlag() {
        assertEquals(false, MockPolicy.allowed(debuggable = false, flagFileExists = true))
        assertEquals(false, MockPolicy.allowed(debuggable = true, flagFileExists = false))
        assertEquals(true, MockPolicy.allowed(debuggable = true, flagFileExists = true))
    }

    @Test fun sensorStatusNamesTheCompassAccuracy() {
        assertEquals("high", CompassAccuracies.name(3))
        assertEquals("medium", CompassAccuracies.name(2))
        assertEquals("low", CompassAccuracies.name(1))
        assertEquals("unreliable", CompassAccuracies.name(0))
        assertEquals("unreliable", CompassAccuracies.name(-1))
    }
}
