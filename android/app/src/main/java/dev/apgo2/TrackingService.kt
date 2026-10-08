package dev.apgo2

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.IBinder
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Keeps the app a foreground process while a game is open, so location and step updates (see [Sensors]) keep
 * arriving with the screen off. It holds no logic of its own; the user sees it as an ongoing notification.
 */
class TrackingService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var beat: Job? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(
        intent: Intent?,
        flags: Int,
        startId: Int,
    ): Int {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, "Quest tracking", NotificationManager.IMPORTANCE_LOW))
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val n =
            Notification
                .Builder(this, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_menu_mylocation)
                .setContentTitle("Archipela-Go 2 is tracking your quests")
                .setContentText("Tap to open. Close the game to stop.")
                .setOngoing(true)
                .setContentIntent(open)
                .build()
        startForeground(ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION)
        Diag.i("service", "started", "restart" to (intent == null))
        if (beat?.isActive != true) {
            beat =
                scope.launch {
                    while (true) {
                        delay(HEARTBEAT_MS)
                        (application as ApgoApp).model.heartbeat()
                    }
                }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        Diag.i("service", "stopped")
        scope.cancel()
        super.onDestroy()
    }

    companion object {
        private const val CHANNEL = "tracking"
        private const val ID = 1
        private const val HEARTBEAT_MS = 60_000L

        fun start(ctx: Context) = ctx.startForegroundService(Intent(ctx, TrackingService::class.java))

        fun stop(ctx: Context) {
            ctx.stopService(Intent(ctx, TrackingService::class.java))
        }
    }
}
