package dev.apgo2

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.apgo_ffi.OfferOut
import uniffi.apgo_ffi.ScanListener
import uniffi.apgo_ffi.ScanPlanOut
import java.util.concurrent.ConcurrentHashMap

// At most one download per realm in this time.
private const val SCAN_COOLDOWN_MS = 20_000L

// A scan needing this many downloads asks first (about 20 tiles).
private const val BIG_SCAN_REQUESTS = 60

private const val QUIET_RETRY_MS = 45_000L
private const val MAX_QUIET_RETRIES = 4
private const val SCANNING_TEXT = "Looking for finds…"

/** A scan that needs a lot of downloading and waits for the player's go-ahead. */
internal data class ScanAsk(
    val id: String,
    val requests: Int,
    val tiles: Int,
)

/**
 * Fetches the finds of realms. Anything already downloaded (by this or any other realm) is reused, so a repeated or overlapping
 * scan costs nothing. Downloads are rationed: at most one per realm every [SCAN_COOLDOWN_MS] (a request inside the window is
 * deferred, not lost), and a big area asks first. Trouble reaching the map servers is retried quietly in the background.
 */
internal class ScanCoordinator(
    private val model: AppModel,
    private val scope: CoroutineScope,
) {
    var ask by mutableStateOf<ScanAsk?>(null)

    /** 0..1 while a scan runs; null when it has no measurable progress. */
    var busyFraction by mutableStateOf<Float?>(null)
    private val scanning = mutableSetOf<String>()
    private val waitingToScan = mutableSetOf<String>()
    private val lastDownload = mutableMapOf<String, Long>()
    private val quietRetries = mutableMapOf<String, Int>()

    // Realms deleted (hidden, undo still possible): their scans stop and nothing more is fetched or reported for them. Read by the
    // core on the scanning thread (the listener's keepGoing), so a thread-safe set.
    private val dropped: MutableSet<String> = ConcurrentHashMap.newKeySet()

    // The pending deferred scan or quiet retry of each realm, cancelled when the realm is deleted.
    private val timers = mutableMapOf<String, Job>()

    // Dropped realms whose scan was cut short, and those of them brought back (Undo) while the stopped scan was still winding down.
    private val interrupted = mutableSetOf<String>()
    private val rescanAfter = mutableSetOf<String>()

    /** Realm [id] was deleted: stop its scan now and forget any scan waiting for it (deferred, retry or a pending question). */
    fun forget(id: String) {
        dropped.add(id)
        if (id in scanning || waitingToScan.remove(id) || quietRetries.remove(id) != null) interrupted.add(id)
        timers.remove(id)?.cancel()
        rescanAfter.remove(id)
        if (ask?.id == id) ask = null
        model.engine.cancelScan(id)
    }

    /** Realm [id] came back (Undo): scan it again if its scan was cut short; cached pieces make that quick. */
    fun restore(id: String) {
        dropped.remove(id)
        if (!interrupted.remove(id)) return
        if (id in scanning) rescanAfter.add(id) else scan(id, confirmed = true)
    }

    /** Scan realm [id] in the background; [confirmed] skips the "this is a big download" question. */
    fun scan(
        id: String,
        confirmed: Boolean = false,
    ) {
        scope.launch { scanNow(id, confirmed) }
    }

    private suspend fun scanNow(
        id: String,
        confirmed: Boolean,
    ) {
        if (id in scanning || id in dropped) return
        val plan = withContext(Dispatchers.IO) { model.engine.scanPlan(id) }
        if (id in dropped || (plan.missing > 0u && holdBack(id, plan, confirmed))) return // deleted while the plan was read
        scanning.add(id)
        val result = runWithProgress(id)
        scanning.remove(id)
        model.busy = null
        busyFraction = null
        // A deleted realm's scan was stopped: no message and no retry. One brought back meanwhile is scanned again.
        when {
            id in dropped -> Unit
            rescanAfter.remove(id) -> scan(id, confirmed = true)
            else -> report(id, plan, result)
        }
    }

    private fun report(
        id: String,
        plan: ScanPlanOut,
        result: Result<List<OfferOut>>,
    ) {
        if (plan.missing > 0u) lastDownload[id] = model.now()
        model.realms = model.engine.realms()
        showOutcome(id, result)
        // Pieces that did not arrive (or a failure) are picked up again later, a few times, without bothering the player.
        val partial = result.isFailure || model.realms.firstOrNull { it.id == id }?.warning != null
        retryQuietly(id, partial)
    }

    // True when this scan must not start now: it is deferred by the cooldown, or waits for the player's go-ahead.
    private fun holdBack(
        id: String,
        plan: ScanPlanOut,
        confirmed: Boolean,
    ): Boolean {
        val wait = SCAN_COOLDOWN_MS - (model.now() - (lastDownload[id] ?: 0L))
        return when {
            wait > 0 -> {
                deferScan(id, wait, confirmed)
                true
            }

            needsAnswer(plan, confirmed) -> {
                ask = ScanAsk(id, plan.missing.toInt(), plan.tiles.toInt())
                true
            }

            else -> {
                false
            }
        }
    }

    private fun needsAnswer(
        plan: ScanPlanOut,
        confirmed: Boolean,
    ) = plan.missing >= BIG_SCAN_REQUESTS.toUInt() && !confirmed

    private fun deferScan(
        id: String,
        wait: Long,
        confirmed: Boolean,
    ) {
        if (!waitingToScan.add(id)) return
        timers[id] =
            scope.launch {
                delay(wait)
                waitingToScan.remove(id)
                timers.remove(id)
                scan(id, confirmed)
            }
    }

    private suspend fun runWithProgress(id: String): Result<List<OfferOut>> {
        model.busy = SCANNING_TEXT
        busyFraction = null
        // The core calls this as each map request finishes (on the scanning thread): no polling.
        val listener =
            object : ScanListener {
                override fun progress(
                    done: UInt,
                    total: UInt,
                ) {
                    if (total == 0u) return
                    scope.launch(Dispatchers.Main) {
                        busyFraction = done.toFloat() / total.toFloat()
                        model.busy = "$SCANNING_TEXT $done of $total"
                    }
                }

                // Asked by the core before each map request: a realm hidden for deletion stops at once.
                override fun keepGoing(): Boolean = id !in dropped
            }
        return withContext(Dispatchers.IO) { runCatching { model.engine.scanRealm(id, model.now().toULong(), listener) } }
    }

    private fun showOutcome(
        id: String,
        result: Result<List<OfferOut>>,
    ) {
        result.onSuccess {
            model.offers[id] = it
            model.library.refreshStreets(id) // an open game playing in this realm gets the new scan's streets
            model.status = ""
        }
        result.onFailure {
            Diag.failure("scan", it)
            model.status = "Couldn't reach the map servers just now. Trying again in a moment."
        }
    }

    private fun retryQuietly(
        id: String,
        partial: Boolean,
    ) {
        val tries = quietRetries[id] ?: 0
        if (partial && tries < MAX_QUIET_RETRIES) {
            quietRetries[id] = tries + 1
            timers[id] =
                scope.launch {
                    delay(QUIET_RETRY_MS)
                    timers.remove(id)
                    scan(id, confirmed = true)
                }
        } else if (!partial) {
            quietRetries.remove(id)
        }
    }
}
