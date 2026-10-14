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
import uniffi.apgo_ffi.Engine
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

        /** The recap was closed by the arrival at [arrivedMs]; an arrival with no walk to close keeps the older one. */
        fun isNew(
            recap: RecapOut,
            arrivedMs: Long,
        ) = recap.endMs == arrivedMs
    }
}

/** Holds the pending walk recap for Compose (the core owns it) and posts the arrival notice. */
internal class RecapController(
    ctx: Context,
    private val engine: Engine,
    private val presence: PresenceController,
) {
    private val notice = RecapNotice(ctx)

    /** The recap on show, until dismissed. */
    var recap by mutableStateOf<RecapOut?>(null)
        private set

    /** Arrived home at [arrivedMs]: show the walk's recap, or notify when off screen. An older recap is not shown or posted again. */
    fun onArrivedHome(arrivedMs: Long) {
        val r = engine.pendingRecap() ?: return
        if (!RecapNotice.isNew(r, arrivedMs)) return
        recap = r
        if (RecapNotice.shouldPost(presence.appVisible)) notice.post(r)
    }

    /** Re-read the core's pending recap (the app came back to the screen, or a game opened). */
    fun refresh() {
        recap = engine.pendingRecap()
    }

    fun dismiss() {
        engine.dismissRecap()
        recap = null
        notice.cancel()
    }
}
