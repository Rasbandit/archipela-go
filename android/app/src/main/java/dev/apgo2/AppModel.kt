package dev.apgo2

import android.content.Context
import android.location.Location
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.apgo2.presence.PresenceSettings
import kotlinx.coroutines.CoroutineScope
import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.AuditEventOut
import uniffi.apgo_ffi.ChainOut
import uniffi.apgo_ffi.Engine
import uniffi.apgo_ffi.EventOut
import uniffi.apgo_ffi.GameInfo
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.HudOut
import uniffi.apgo_ffi.OfferOut
import uniffi.apgo_ffi.QuestOut
import uniffi.apgo_ffi.RealmOut
import uniffi.apgo_ffi.ZoneOut
import kotlin.random.Random

// Reloading the whole trace on every fix gets slower as it grows; every 10 s is plenty for a line on a map.
private const val TRACE_REFRESH_MS = 10_000L

// Step readings without events refresh the Play screen only once the count moved this much (a visible change).
private const val STEP_REFRESH_STEPS = 50L
private const val LOG_LIMIT = 60
private const val EVENT_LOG_CHARS = 300

/** The bottom-bar tabs, by index. */
internal object AppTab {
    const val PLAY = 0
    const val REALMS = 1
    const val NEW_GAME = 2
    const val ACTIVITY = 3
    const val SETTINGS = 4
}

/**
 * The app's state, shared by every screen, and the engine behind it. Work is split over the collaborators it owns: [presence]
 * (is the player playing), [scans], [realmOps], [library] (games), [ap] (Archipelago), [setup], [units], [sim] and [diag].
 */
internal class AppModel(
    private val ctx: Context,
    private val scope: CoroutineScope,
) {
    val engine = Engine(ctx.filesDir.absolutePath)
    val sensors = Sensors(ctx, this)
    val settings = PresenceSettings(ctx)
    val units = UnitSettings(this)
    val presence = PresenceController(this, ctx)
    val setup = SetupWizard(this)
    val scans = ScanCoordinator(this, scope)
    val realmOps = RealmManager(this)
    val library = GameLibrary(this, scope)
    val ap = ApController(this, ctx, scope)
    val sim = DevSimulator(this, scope)
    val diag = FieldDiagnostics(this, ctx)
    val due = DueTimer(this, scope, ctx)
    private val traceThrottle = Throttle(TRACE_REFRESH_MS)
    private val stepRefresh = StepRefresh(STEP_REFRESH_STEPS)

    /** The tab showing, one of [AppTab]. */
    var tab by mutableIntStateOf(AppTab.PLAY)

    /** The realm editor: null shows the realm list, "" a new realm, otherwise the id of the realm being edited. */
    var editing by mutableStateOf<String?>(null)
    var realms by mutableStateOf<List<RealmOut>>(emptyList())
    val offers = mutableStateMapOf<String, List<OfferOut>>()
    var busy by mutableStateOf<String?>(null)
    var status by mutableStateOf("")
    var quests by mutableStateOf<List<QuestOut>>(emptyList())
    var zones by mutableStateOf<List<ZoneOut>>(emptyList())
    var hud by mutableStateOf<HudOut?>(null)

    /** Where you have been in this game: one line per unbroken stretch of GPS. */
    var trace by mutableStateOf<List<List<LatLng>>>(emptyList())

    var games by mutableStateOf<List<GameInfo>>(emptyList())

    /** What happened in the open (or last paused) game, newest first; see [GameLibrary.refreshActivity]. */
    var activity by mutableStateOf<List<AuditEventOut>>(emptyList())

    /** Moves whenever the engine wrote to the activity log; the Activity tab reloads on a change instead of on a timer. */
    var journalRev by mutableLongStateOf(0L)
        private set
    val log = mutableStateListOf<String>()
    var realLoc by mutableStateOf<Location?>(null)
    var simPos by mutableStateOf<LatLng?>(null)
    var home by mutableStateOf<GeoPoint?>(null)
    val draft = mutableStateListOf<LatLng>()
    var selected by mutableStateOf<Long?>(null)
    var chains by mutableStateOf<List<ChainOut>>(emptyList())
    var selectedChain by mutableStateOf<String?>(null)
    var yamlText by mutableStateOf<String?>(null)

    /** Cumulative steps since boot from the phone's step counter (null when unavailable or not permitted). */
    var stepsTotal by mutableStateOf<Long?>(null)

    /** Surface preference: any, prefer_paved or paved_only. */
    var surfacePref by mutableStateOf("any")
    var avoidStairs by mutableStateOf(false)
    val me: LatLng?
        get() = simPos ?: realLoc?.let { LatLng(it.latitude, it.longitude) }

    /** The current time in ms. */
    fun now() = System.currentTimeMillis()

    /** An operation ([what]) threw: log it with its stack and show "[text]: reason" in the status line. */
    fun fail(
        what: String,
        text: String,
        t: Throwable,
    ) {
        Diag.failure(what, t)
        status = "$text: ${t.message}"
    }

    /** Add a line to the on-screen log, newest first. */
    fun say(msg: String) {
        log.add(0, msg)
        while (log.size > LOG_LIMIT) log.removeAt(log.lastIndex)
    }

    /** Reload everything shown from the engine. */
    fun refreshAll() {
        realms = engine.realms()
        home = engine.home()
        games = engine.games()
        realms.forEach { offers[it.id] = engine.realmOffers(it.id) }
        refreshPlay()
        presence.evaluate() // the `playing` signal follows `hud`, which every game start, open and pause goes through here
    }

    /** Reload the open game's quests, zones, chains and HUD (and its trace, when [withTrace]). */
    fun refreshPlay(withTrace: Boolean = true) {
        if (engine.hasGame()) {
            quests = engine.quests(now())
            zones = engine.zones()
            chains = engine.chains(now())
            hud = engine.hud(now())
            if (withTrace) trace = engine.track(0L, Long.MAX_VALUE).map { seg -> seg.points.map { LatLng(it.lat, it.lon) } }
        } else {
            quests = emptyList()
            zones = emptyList()
            chains = emptyList()
            selectedChain = null
            hud = null
            trace = emptyList()
        }
        noteJournal()
        due.schedule()
    }

    /** Pick up whether the activity log changed (a cheap read; call after anything that may have logged). */
    fun noteJournal() {
        journalRev = engine.journalRevision().toLong()
    }

    /** The phone's step counter changed: credit it to the open game (the engine ignores it when no game is open). */
    fun onSteps(total: Long) {
        stepsTotal = total
        if (!engine.hasGame()) return
        val events = engine.onSteps(total, now())
        handle(events)
        // The counter reports in batches: refresh when something happened or the count moved enough to show, never on a timer.
        if (events.isNotEmpty() || stepRefresh.due(total, engine.openGameId())) refreshPlay(withTrace = false)
    }

    /** A location fix arrived: feed the engine, update presence and the screen. */
    fun onFix(loc: Location) {
        if (!engine.hasGame() || simPos != null) return
        diag.recordFix(loc)
        handle(engine.onFix(loc.latitude, loc.longitude, now(), loc.accuracy.toDouble(), stepsTotal, false))
        presence.updateZone(engine.lastZoneProximity())
        presence.evaluate()
        refreshPlay(withTrace = traceThrottle.due(now()))
        diag.logProgress()
    }

    /** The app left the screen: the trace has a gap from now on. */
    fun onBackground() {
        engine.saveGame()
        engine.logAppState(false, now())
        noteJournal()
        Diag.info("lifecycle", "background")
        diag.drainCore()
    }

    /** Back on screen: log it, and how long the app was away. */
    fun onForeground() {
        val left = engine.lastBackgroundMs()
        val t = now()
        engine.logAppState(true, t)
        noteJournal()
        Diag.info("lifecycle", "foreground", "away_ms" to (left?.let { t - it } ?: -1L))
    }

    /** Show what the engine reported, and pass on to Archipelago what it needs to hear. */
    fun handle(events: List<EventOut>) {
        noteJournal()
        events.forEach { Diag.info("event", it.toString().take(EVENT_LOG_CHARS)) }
        for (e in events) {
            when (e) {
                is EventOut.QuestDone -> {
                    say("Done: ${e.name}")
                }

                is EventOut.Reward -> {
                    say("Found: ${e.item}")
                }

                is EventOut.ZoneUnlocked -> {
                    say("Zone ${e.zone} unlocked!")
                }

                is EventOut.Trap -> {
                    say(e.message)
                }

                is EventOut.Discovered -> {}

                is EventOut.GoalAchieved -> {
                    onGoal(e.label)
                }

                is EventOut.Info -> {
                    say(e.text)
                }

                is EventOut.SendCheck -> {
                    report("ap_check", "check failed") { ap.session?.sendCheck(e.locationId) }
                }

                is EventOut.ShuffleRequested -> {
                    runCatching { engine.reroll(emptyList(), Random.nextLong().toULong() shr 1) }
                }
            }
        }
    }

    private fun onGoal(label: String) {
        say("GOAL ACHIEVED: $label")
        status = "You won! $label"
        if (hud?.backend == "archipelago") report("ap_goal", "could not report goal") { ap.session?.sendGoal() }
    }

    // Run an Archipelago call; if it throws, log it and tell the player in the on-screen log.
    private fun report(
        what: String,
        message: String,
        call: () -> Unit,
    ) {
        runCatching(call).onFailure {
            Diag.failure(what, it)
            say("$message: ${it.message}")
        }
    }
}
