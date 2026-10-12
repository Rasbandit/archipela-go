package dev.apgo2

import android.annotation.SuppressLint
import android.content.Context
import android.location.GnssStatus
import android.location.LocationManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock

// The longest a GPS batch waits for one satellite status (the chip fixes every 5 s off screen and reports while it runs).
private const val SAMPLE_MAX_MS = 3_000L

/** Satellites used in a fix in this status. */
internal val GnssStatus.usedCount: Int get() = (0 until satelliteCount).count { usedInFix(it) }

/** Register [cb] for satellite status on the main thread. */
@SuppressLint("MissingPermission")
internal fun LocationManager.registerStatus(
    ctx: Context,
    cb: GnssStatus.Callback,
) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        registerGnssStatusCallback(ctx.mainExecutor, cb)
    } else {
        @Suppress("DEPRECATION") // the Handler overload is the only one before Android 11
        registerGnssStatusCallback(cb, Handler(Looper.getMainLooper()))
    }
}

/**
 * Batches waiting for one satellite-status sample ([GnssSampler]): the first request starts the sample, its end releases them all,
 * in arrival order. Not thread-safe: use it from the main thread.
 */
internal class SampleGate {
    private val waiting = mutableListOf<() -> Unit>()

    /**
     * Whether a batch may be counted at once: only with the full status callback running ([continuous]) and no older batch held,
     * which it must not overtake (the core would take its older fixes as current).
     */
    fun bypass(continuous: Boolean): Boolean = continuous && waiting.isEmpty()

    /** Queue [onDone]; true when this request should start a sample (none is in flight). */
    fun request(onDone: () -> Unit): Boolean {
        waiting += onDone
        return waiting.size == 1
    }

    /** The sample ended (a status arrived or it timed out): run every waiting batch. */
    fun complete() {
        val ready = waiting.toList()
        waiting.clear()
        ready.forEach { it() }
    }
}

/**
 * Off screen the satellite-status callback does not run (its 1 Hz reports would keep the CPU awake through batching), so a fused
 * Wi-Fi fix could pass as GPS (adversarial review I2). Instead each batched in-zone GPS batch ([GpsPolicy.samplesSatellites]) waits
 * for one status (at most [SAMPLE_MAX_MS]) before it is counted; [continuous] says the full callback runs (map in front). The timeout
 * is not a wake-up alarm: with the CPU asleep it ends at the next wake, within about 5 s in a zone (the wake-up compass and steps).
 */
internal class GnssSampler(
    private val ctx: Context,
    private val lm: LocationManager,
    private val evidence: GnssEvidence,
    private val continuous: () -> Boolean,
) {
    private val main = Handler(Looper.getMainLooper())
    private val gate = SampleGate()
    private var callback: GnssStatus.Callback? = null
    private var sampling = false
    private var startedMs = 0L
    private val timeout = Runnable { end(got = false) }

    /** Run [then] once the satellite evidence is fresh: at once while the full callback runs, else after one sample. */
    fun withEvidence(then: () -> Unit) {
        if (gate.bypass(continuous())) return then()
        if (!gate.request(then)) return
        sampling = true
        startedMs = SystemClock.elapsedRealtime()
        val cb =
            object : GnssStatus.Callback() {
                override fun onSatelliteStatusChanged(status: GnssStatus) {
                    evidence.onStatus(status.usedCount, SystemClock.elapsedRealtime())
                    if (callback === this) end(got = true) // a late report of an ended sample must not end the next one
                }
            }
        if (runCatching { lm.registerStatus(ctx, cb) }.isSuccess) {
            callback = cb
            main.postDelayed(timeout, SAMPLE_MAX_MS)
        } else {
            end(got = false)
        }
    }

    /** Run [then] at once, or after a held batch if one is waiting (a rate change must not let newer fixes overtake it). */
    fun afterHeld(then: () -> Unit) {
        if (sampling) gate.request(then) else then()
    }

    /** Location stopped: count any held batch now (its fixes were taken before the stop) and stop listening. */
    fun release() = end(got = false)

    // A status arrived ([got]) or the wait timed out: stop listening and count the waiting batches (with no status, fused is trusted).
    private fun end(got: Boolean) {
        if (!sampling) return
        sampling = false
        // Never left holding batches: a failed unregister must not keep every later fix waiting.
        callback?.let { cb -> runCatching { lm.unregisterGnssStatusCallback(cb) } }
        callback = null
        main.removeCallbacks(timeout)
        Diag.info("gnss", "sample", "status" to got, "waited_ms" to SystemClock.elapsedRealtime() - startedMs)
        gate.complete()
    }
}
