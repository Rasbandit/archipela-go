package dev.apgo2

import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.RecapChainOut
import uniffi.apgo_ffi.RecapOut
import uniffi.apgo_ffi.RecapQuestOut
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

/** Made-up walk for the debug "Preview walk recap" button: lets the recap animations be iterated on without walking. */
internal object RecapSample {
    private const val MIN_MS = 60_000L
    private const val WALK_MIN = 42L
    private const val LOOP_POINTS = 12
    private const val LAT_SPAN = 0.0018
    private const val LON_SPAN = 0.0024
    private const val STEPS = 4812L
    private const val COUNTED_STEPS = 4090L
    private const val DISTANCE_M = 3100.0
    private const val NEW_CELLS = 14u
    private const val BANKED = 3u
    private const val ZONE = 1u

    private val CHAINS =
        listOf(
            chain("step_up", "Step Up", "steps", 2140.0, 6230.0, listOf(1000.0, 2500.0, 5000.0, 7500.0, 10_000.0)),
            chain("wanderlust", "Wanderlust", "minutes", 48.0, 90.0, listOf(30.0, 60.0, 120.0, 240.0)),
            chain("cartographer", "Cartographer", "cells", 31.0, 45.0, listOf(10.0, 25.0, 40.0, 100.0)),
        )

    /** A quest on the route: name, place, goal, reward, route index, and how far through the walk it was done (percent). */
    private class Spec(
        val name: String,
        val place: String,
        val goal: String,
        val reward: String?,
        val routeIndex: Int,
        val percent: Int,
    )

    private val SPECS =
        listOf(
            Spec("Reach the fountain", "City Park", "Reach City Park", "Hydrate!", 2, 25),
            Spec("Rest a while", "Library steps", "Stay 2 min at Library steps", "Progressive Zone Key", 6, 55),
            Spec("Trail sampler", "River trail", "Walk 300 m of River trail", null, 9, 80),
        )

    fun build(
        center: GeoPoint,
        nowMs: Long,
    ): RecapOut {
        val start = nowMs - WALK_MIN * MIN_MS
        val loop = closedLoop(center)
        val quests =
            SPECS.map { s ->
                RecapQuestOut(s.name, s.place, s.goal, start + (nowMs - start) * s.percent / 100, loop[s.routeIndex], s.reward)
            }
        return RecapOut(
            startMs = start,
            endMs = nowMs,
            steps = STEPS,
            distanceM = DISTANCE_M,
            newCells = NEW_CELLS,
            questsDone = quests.map { it.name },
            countedSteps = COUNTED_STEPS,
            chains = CHAINS,
            quests = quests,
            checksSent = 0u,
            items = SPECS.mapNotNull { it.reward },
            zonesUnlocked = listOf(ZONE + 1u),
            banked = BANKED,
            route = listOf(loop),
        )
    }

    private fun chain(
        id: String,
        name: String,
        unit: String,
        before: Double,
        after: Double,
        marks: List<Double>,
    ) = RecapChainOut("$ZONE:$id", name, unit, ZONE, before, after, marks)

    // A closed ellipse around [c]: the last point repeats the first.
    private fun closedLoop(c: GeoPoint): List<GeoPoint> {
        val ring =
            (0 until LOOP_POINTS).map { i ->
                val a = 2 * PI * i / LOOP_POINTS
                GeoPoint(c.lat + LAT_SPAN * sin(a), c.lon + LON_SPAN * cos(a))
            }
        return ring + ring.first()
    }
}
