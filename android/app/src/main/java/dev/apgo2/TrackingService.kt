package dev.apgo2

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder

private const val TAG = "service"

/** What a start of [TrackingService] has to do besides showing its notification. */
internal enum class ServiceStart {
    /** Started by the app: the activity's effects run presence, steps and GPS. */
    FromApp,

    /** Restarted by Android with a game open and no screen: start tracking from the service. */
    ResumeHeadless,

    /** Restarted with no game to track, or no location access from the background: stop, so no notification lingers. */
    Stop,
    ;

    companion object {
        /** [backgroundLocation]: "Allow all the time"; a restart from the background gets no fixes with "while using the app". */
        fun decide(
            restarted: Boolean,
            playing: Boolean,
            backgroundLocation: Boolean,
        ) = when {
            !restarted -> FromApp
            playing && backgroundLocation -> ResumeHeadless
            else -> Stop
        }
    }
}

/**
 * Keeps the app a foreground process while a game is open, so location and step updates (see [Sensors]) keep
 * arriving with the screen off. It holds no logic of its own; the user sees it as an ongoing notification.
 */
class TrackingService : Service() {
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
        // A location service started from the background without location access is refused (SecurityException, Android 14+).
        val started =
            runCatching {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    startForeground(ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION)
                } else {
                    startForeground(ID, n) // the foreground service type only exists from Android 10
                }
            }.onFailure { Diag.error(TAG, "foreground start refused", it) }
        if (started.isFailure) {
            stopSelf()
            return START_NOT_STICKY
        }
        val restarted = intent == null
        Diag.info(TAG, "started", "restart" to restarted)
        // A restart by Android (START_STICKY) comes with no activity: loading the model resumes the saved game, then tracking is
        // started here instead of by the screen's effects.
        val model = (application as ApgoApp).model
        when (ServiceStart.decide(restarted, playing = model.hud != null, backgroundLocation = hasBackgroundLocation())) {
            ServiceStart.FromApp -> Unit
            ServiceStart.ResumeHeadless -> resumeWithoutScreen(model)
            ServiceStart.Stop -> stopSelf()
        }
        return START_STICKY
    }

    // What the activity's effects would start: steps, presence and GPS.
    private fun resumeWithoutScreen(model: AppModel) {
        Diag.info(TAG, "resume without screen")
        if (hasActivityRecognition()) model.sensors.startSteps()
        model.presence.startHeadless(hasFineLocation(), hasBluetoothConnect())
    }

    override fun onDestroy() {
        Diag.info(TAG, "stopped")
        super.onDestroy()
    }

    /** Constants and the start/stop entry points for the service. */
    companion object {
        private const val CHANNEL = "tracking"
        private const val ID = 1

        /** Starts the foreground service. */
        fun start(ctx: Context) = ctx.startForegroundService(Intent(ctx, TrackingService::class.java))

        /** Stops the foreground service. */
        fun stop(ctx: Context) {
            ctx.stopService(Intent(ctx, TrackingService::class.java))
        }
    }
}
