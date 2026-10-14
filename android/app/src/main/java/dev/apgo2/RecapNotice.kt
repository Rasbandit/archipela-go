package dev.apgo2

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.apgo2.ui.Help
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.apgo_ffi.Engine
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.RecapOut

/** "Welcome home: see your walk": posted when a walk's recap is ready while the app is off screen. Opening the app shows it. */
internal class RecapNotice(
    private val ctx: Context,
) {
    private val nm = ctx.getSystemService(NotificationManager::class.java)

    init {
        val channel =
            NotificationChannel(CHANNEL, "Walk recap", NotificationManager.IMPORTANCE_DEFAULT).apply {
                description = "How your walk went, when you get home"
                enableVibration(false)
                setSound(null, null)
            }
        nm.createNotificationChannel(channel)
    }

    fun post(recap: RecapOut) {
        if (!nm.areNotificationsEnabled()) return
        val intent = Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        val open = PendingIntent.getActivity(ctx, ID, intent, PendingIntent.FLAG_IMMUTABLE)
        val n =
            Notification
                .Builder(ctx, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setContentTitle(Help.RECAP_NOTICE_TITLE)
                .setContentText(Help.recapNoticeText(recap))
                .setAutoCancel(true)
                .setContentIntent(open)
                .build()
        nm.notify(ID, n)
    }

    fun cancel() = nm.cancel(ID)

    companion object {
        private const val CHANNEL = "walk_recap_v1"
        private const val ID = 3000

        /** Post only while the app is off screen. */
        fun shouldPost(appVisible: Boolean) = !appVisible
    }
}

/** Holds the pending walk recap for Compose (the core owns it) and posts the arrival notice. */
internal class RecapController(
    ctx: Context,
    private val engine: Engine,
    private val presence: PresenceController,
    private val scope: CoroutineScope,
) {
    private val notice = RecapNotice(ctx)

    /** The recap on show, until dismissed. */
    var recap by mutableStateOf<RecapOut?>(null)
        private set

    // A preview or a past walk: not the core's pending recap, so closing it never touches the core.
    private var transient = false

    /** What the recap screen's button says. */
    val closeLabel: String get() = closeLabel(transient)

    /** The core closed a walk into a new recap (`RecapReady`): show it, or notify when off screen. It is announced once. */
    fun onRecapReady() {
        val r = engine.pendingRecap() ?: return
        transient = false
        recap = r
        if (RecapNotice.shouldPost(presence.appVisible)) notice.post(r)
    }

    /** Re-read the core's pending recap (the app came back to the screen, or a game opened). */
    fun refresh() {
        transient = false
        recap = engine.pendingRecap()
    }

    /** Debug only: show [r] (sample data) without touching the core. Dismissing it leaves any real pending recap alone. */
    fun preview(r: RecapOut) {
        transient = true
        recap = r
    }

    /** Show a past walk's [r] from the activity log. It never replaces a real pending recap and never touches the core on close. */
    fun showHistory(r: RecapOut) {
        if (!historyMayShow(shown = recap != null, transient = transient)) return
        transient = true
        recap = r
    }

    /** Open the recap of the walk that ended at [endMs]. The journal read runs off the main thread. */
    fun openWalk(endMs: Long) {
        scope.launch {
            val r = withContext(Dispatchers.IO) { runCatching { engine.walkRecap(endMs) } }
            r.onFailure { Diag.error("recap", "open walk failed", it) }
            r.getOrNull()?.let(::showHistory)
        }
    }

    fun dismiss() {
        if (dismissTouchesCore(transient)) {
            engine.dismissRecap()
            notice.cancel()
        }
        transient = false
        recap = null
    }

    companion object {
        /** A previewed or past recap is not the core's pending one, so dismissing it must not clear that. */
        fun dismissTouchesCore(transient: Boolean) = !transient

        fun closeLabel(transient: Boolean) = if (transient) Help.RECAP_CLOSE else Help.RECAP_DONE

        /** History may open over nothing or another transient recap, never over a real pending one. */
        fun historyMayShow(
            shown: Boolean,
            transient: Boolean,
        ) = !shown || transient
    }
}

/** Debug builds: show the recap screen with made-up data around home (or the last place), saving nothing. */
internal fun AppModel.previewRecap() {
    val centre = realmOps.homePoint() ?: lastPlace?.let { GeoPoint(it.latitude, it.longitude) } ?: GeoPoint(40.7608, -111.8910)
    recaps.preview(RecapSample.build(centre, now()))
}
