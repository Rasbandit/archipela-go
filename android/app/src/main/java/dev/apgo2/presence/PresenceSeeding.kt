package dev.apgo2.presence

/** Which debouncers to seed right now. */
data class SeedPlan(val home: Boolean, val car: Boolean)

/** Decides when each presence debouncer takes its first reading after a monitor (re)start. Pure: the caller supplies the clock. */
class PresenceSeeding(private val timeoutMs: Long = 3_000) {
    private var started = false
    private var startMs = 0L
    private var homeDone = false
    private var carDone = false

    /** A monitor (re)start: nothing is seeded, the timeout counts from [nowMs]. */
    fun restart(nowMs: Long) {
        started = true
        startMs = nowMs
        homeDone = false
        carDone = false
    }

    /** A signal is due when its own reading arrived or the timeout has passed; each is returned at most once per restart. */
    fun poll(nowMs: Long, wifiReported: Boolean, bluetoothReady: Boolean): SeedPlan {
        if (!started) return SeedPlan(home = false, car = false)
        val timedOut = nowMs - startMs >= timeoutMs
        val home = !homeDone && (wifiReported || timedOut)
        val car = !carDone && (bluetoothReady || timedOut)
        homeDone = homeDone || home
        carDone = carDone || car
        return SeedPlan(home, car)
    }

    val complete: Boolean get() = homeDone && carDone

    /** A restart happened and not everything is seeded yet; false when the monitor was never started. */
    val waiting: Boolean get() = started && !complete
}
