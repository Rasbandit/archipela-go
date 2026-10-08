package dev.apgo2.presence

enum class Zone { Inside, Near, Far, Unknown }

enum class PresenceState { Stopped, InCar, AtHome, InZone, OutsideZones }

/** What the phone currently knows. `null` for a signal means unavailable or not permitted: treated as "not present". */
data class Signals(val playing: Boolean, val homeWifi: Boolean?, val carBluetooth: Boolean?, val zone: Zone)

sealed interface GpsMode {
    object Off : GpsMode
    data class Rate(val intervalMs: Long, val minDistanceM: Float) : GpsMode
}

data class Decision(val state: PresenceState, val gps: GpsMode, val counting: Boolean)

/** Presence rules (spec Part B): which state the player is in decides how GPS runs and whether progress counts. First match wins. */
object PresencePolicy {
    const val COARSE_MS = 90_000L
    private const val PRECISE_MS = 5_000L

    fun decide(s: Signals): Decision = when {
        !s.playing -> Decision(PresenceState.Stopped, GpsMode.Off, counting = false)
        s.carBluetooth == true -> Decision(PresenceState.InCar, GpsMode.Off, counting = false)
        s.homeWifi == true -> Decision(PresenceState.AtHome, GpsMode.Off, counting = false)
        s.zone == Zone.Far -> Decision(PresenceState.OutsideZones, GpsMode.Rate(COARSE_MS, 0f), counting = true)
        else -> Decision(PresenceState.InZone, GpsMode.Rate(PRECISE_MS, 0f), counting = true)
    }
}

/** Holds a signal steady: a change (including to unknown, `null`) only becomes the stable value after it has lasted [holdMs] (Wi-Fi reaches past the door, Bluetooth flaps). The very first value is adopted at once. */
class Debouncer(private val holdMs: Long = 45_000) {
    private var stable: Boolean? = null
    private var started = false
    private var candidate: Boolean? = null
    private var since = 0L

    /** True while the latest raw value differs from the stable one, so a later [feed] may still change the outcome. */
    val pending: Boolean get() = started && candidate != stable

    private var adoptNext = false

    /**
     * Forget the history and start from [value]: a non-null value is stable at once and later changes are debounced; `null` means
     * "no history: the next real value is adopted at once". That differs from a fresh debouncer fed `null`, which stays unknown but
     * would hold a following value. Never pending right after.
     */
    fun seed(value: Boolean?) {
        started = true
        stable = value
        candidate = value
        adoptNext = value == null
    }

    fun feed(raw: Boolean?, nowMs: Long): Boolean? {
        if (adoptNext && raw != null) {
            adoptNext = false
            stable = raw
            candidate = raw
            since = nowMs
            return stable
        }
        if (!started) {
            started = true
            stable = raw
            candidate = raw
            since = nowMs
            return stable
        }
        if (raw != candidate) {
            candidate = raw
            since = nowMs
        }
        if (candidate != stable && nowMs - since >= holdMs) stable = candidate
        return stable
    }
}
