package dev.apgo2

import android.annotation.SuppressLint
import dev.apgo2.ui.METERS_PER_DEGREE
import dev.apgo2.ui.METERS_PER_KM
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.EventOut
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.QuestOut
import kotlin.math.ceil
import kotlin.math.cos
import kotlin.math.hypot

private const val MINUTE_MS = 60_000L
private const val GAP_MS = 10 * MINUTE_MS
private const val SIM_ACCURACY_M = 5.0
private const val START_STEPS = 50_000L
private const val STEPS_PER_FIX = 400
private const val MAX_TRAP_ESCAPES = 4

// A walk along a row of cells (and a toll's detour): one fix every 90 s, 160 m further east.
private const val CELL_FIX_MS = 90_000L
private const val CELL_STRIDE_M = 160.0
private const val CELL_FIXES = 70
private const val TOLL_FIXES = 8

private const val DWELL_MS = 11 * MINUTE_MS
private const val ARRIVAL_MS = 400_000L
private const val LINE_FIX_MS = 15_000L
private const val LINE_SPACING_M = 10.0
private const val STEP_FIXES = 30
private const val DEFAULT_LIMIT_MIN = 20L

// The game only counts gaps up to 5 minutes, so the away quest is fed fixes 4 minutes apart.
private const val AWAY_GAP_MIN = 4
private const val AWAY_EXTRA_FIXES = 3
private const val AWAY_MARGIN_M = 600.0
private const val DEFAULT_AWAY_KM = 2.0
private const val DEFAULT_AWAY_MIN = 60
private const val LOGGED_EVENTS = 4
private const val TRAP_KIND_TOLL = "Toll"
private const val TRAP_KIND_LEASH = "Leash"

/** Feeds the engine the kind of fix sequence a real player would produce to complete a quest, for testing without walking. */
internal class DevSimulator(
    private val model: AppModel,
    private val scope: CoroutineScope,
) {
    private var clockMs = 0L
    private var steps = START_STEPS

    /** A new game starts the virtual clock over. */
    fun resetClock() {
        clockMs = 0
    }

    /** Complete [q] the way a real player would, then show the events it produced. */
    @SuppressLint("LogNotTimber") // the app does not use Timber; this is the dev simulator's own log line
    fun complete(q: QuestOut) {
        scope.launch(Dispatchers.Default) {
            val anchor = q.anchor
            val home = model.realmOps.homePoint() ?: anchor ?: return@launch
            val out = mutableListOf<EventOut>()
            escapeTraps(home, out)
            out += fixesFor(q, home)
            android.util.Log.i(
                "apgo",
                "sim ${q.name} shape=${q.shape} path=${q.path.size} state=${q.state} -> ${out.size} events ${out.take(LOGGED_EVENTS)}",
            )
            withContext(Dispatchers.Main) {
                model.handle(out)
                model.refreshPlay()
            }
        }
    }

    /** Walk to the next quest that can be reached and complete it. */
    fun teleportNext() {
        val quests = model.quests
        // Fog: undiscovered quests are not "open", but a walker heading for one discovers it on arrival.
        val next = quests.firstOrNull { it.state == "open" || it.state == "progress" } ?: quests.firstOrNull { it.state == "hidden" }
        if (next == null) {
            model.status = "No open quests (zones may be locked or all done)"
            return
        }
        model.status = "Simulating: ${next.name}"
        complete(next)
    }

    // Virtual clock for the simulator: always moves forward, so dwell and trap timers behave.
    private fun tick(ms: Long): Long {
        clockMs = maxOf(clockMs, model.now()) + ms
        return clockMs
    }

    private fun fix(
        p: GeoPoint,
        advanceMs: Long,
        withSteps: Boolean = false,
    ): List<EventOut> {
        model.simPos = LatLng(p.lat, p.lon)
        if (withSteps) steps += STEPS_PER_FIX
        return model.engine.onFix(p.lat, p.lon, tick(advanceMs), SIM_ACCURACY_M, if (withSteps) steps else null, true)
    }

    private fun offset(
        p: GeoPoint,
        northM: Double,
        eastM: Double,
    ) = GeoPoint(p.lat + northM / METERS_PER_DEGREE, p.lon + eastM / (METERS_PER_DEGREE * cos(Math.toRadians(p.lat))))

    // A walk along a row of cells starting at [from].
    private fun walkEast(
        from: GeoPoint,
        fixes: Int,
    ): List<EventOut> {
        var p = from
        return buildList {
            repeat(fixes) {
                addAll(fix(p, CELL_FIX_MS))
                p = offset(p, 0.0, CELL_STRIDE_M)
            }
        }
    }

    // Escape any trap the way a real player would (thaw point, detour waypoint, toll distance, leash).
    private fun escapeTraps(
        home: GeoPoint,
        out: MutableList<EventOut>,
    ) {
        repeat(MAX_TRAP_ESCAPES) {
            val hud = model.engine.hud(model.now()) ?: return
            val thaw = hud.thaw
            val waypoint = hud.waypoint
            when {
                thaw != null -> out += fix(thaw, GAP_MS)
                waypoint != null -> out += fix(waypoint, GAP_MS)
                hud.blocked?.startsWith(TRAP_KIND_TOLL) == true -> out += walkEast(home, TOLL_FIXES)
                hud.blocked?.startsWith(TRAP_KIND_LEASH) == true -> out += fix(home, GAP_MS)
                else -> return
            }
        }
    }

    private fun fixesFor(
        q: QuestOut,
        home: GeoPoint,
    ): List<EventOut> =
        when (q.shape) {
            "line" -> walkLine(q)

            "cells" -> walkEast(home, CELL_FIXES)

            "steps" -> List(STEP_FIXES) { fix(home, MINUTE_MS, withSteps = true) }.flatten()

            // ~6 km/h, like a walk
            "away" -> awayFixes(q, home)

            else -> visitFixes(q, home)
        }

    // Quests that are about being at one or two places.
    private fun visitFixes(
        q: QuestOut,
        home: GeoPoint,
    ): List<EventOut> {
        val anchor = q.anchor
        return when (q.shape) {
            "point", "area" -> anchor?.let { fix(it, GAP_MS) + fix(it, ARRIVAL_MS) }
            "dwell" -> anchor?.let { fix(it, GAP_MS) + fix(it, DWELL_MS) }
            "courier" -> anchor?.let { fix(it, GAP_MS) }.orEmpty() + q.anchorB?.let { fix(it, limitMs(q) / 2) }.orEmpty()
            "roundtrip" -> anchor?.let { fix(it, GAP_MS) }.orEmpty() + fix(home, GAP_MS)
            else -> null
        }.orEmpty()
    }

    private fun walkLine(q: QuestOut): List<EventOut> {
        val out = fix(q.path.first(), GAP_MS).toMutableList()
        for (i in 0 until q.path.size - 1) {
            val a = q.path[i]
            val b = q.path[i + 1]
            val dMeters =
                hypot(
                    (b.lat - a.lat) * METERS_PER_DEGREE,
                    (b.lon - a.lon) * METERS_PER_DEGREE * cos(Math.toRadians(a.lat)),
                )
            val n = ceil(dMeters / LINE_SPACING_M).toInt().coerceAtLeast(1)
            for (k in 1..n) {
                out += fix(GeoPoint(a.lat + (b.lat - a.lat) * k / n, a.lon + (b.lon - a.lon) * k / n), LINE_FIX_MS)
            }
        }
        return out
    }

    private fun awayFixes(
        q: QuestOut,
        home: GeoPoint,
    ): List<EventOut> {
        val km = firstGroup(q.detail, AWAY_KM_PATTERN)?.toDoubleOrNull() ?: DEFAULT_AWAY_KM
        val mins = firstGroup(q.detail, AWAY_MIN_PATTERN)?.toIntOrNull() ?: DEFAULT_AWAY_MIN
        val far = offset(home, km * METERS_PER_KM + AWAY_MARGIN_M, 0.0)
        return fix(far, GAP_MS) + List(mins / AWAY_GAP_MIN + AWAY_EXTRA_FIXES) { fix(far, AWAY_GAP_MIN * MINUTE_MS) }.flatten()
    }

    // The quest's own time limit ("within N min") in ms, so the simulator obeys it like a real player must.
    private fun limitMs(q: QuestOut): Long = (firstGroup(q.detail, LIMIT_PATTERN)?.toLongOrNull() ?: DEFAULT_LIMIT_MIN) * MINUTE_MS

    private fun firstGroup(
        text: String,
        pattern: Regex,
    ) = pattern.find(text)?.groupValues?.get(1)

    private companion object {
        val AWAY_KM_PATTERN = Regex("at least ([0-9.]+) km")
        val AWAY_MIN_PATTERN = Regex("Spend (\\d+) min")
        val LIMIT_PATTERN = Regex("within (\\d+) min")
    }
}
