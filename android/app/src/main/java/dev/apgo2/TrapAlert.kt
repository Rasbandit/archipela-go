package dev.apgo2

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager

/**
 * A trap's loud alert (#160): a heads-up notification on its own high-importance channel, with one long, harsh buzz. The core
 * decides when (trap alerts on, no Silence Trap); this only posts and buzzes. With notifications off it still buzzes.
 */
internal class TrapAlert(
    private val ctx: Context,
) {
    private val nm = ctx.getSystemService(NotificationManager::class.java)

    init {
        val channel =
            NotificationChannel(CHANNEL, "Traps", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "A loud alert and buzz when a trap hits you"
                enableVibration(true)
                vibrationPattern = PATTERN
            }
        nm.createNotificationChannel(channel)
    }

    /** Alert the player that trap [item] hit them, saying what it does ([message]). */
    fun fire(
        item: String,
        message: String,
    ) {
        if (!nm.areNotificationsEnabled()) {
            buzz()
            return
        }
        val open = PendingIntent.getActivity(ctx, 0, Intent(ctx, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val n =
            Notification
                .Builder(ctx, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_dialog_alert)
                .setContentTitle(item)
                .setContentText(message)
                .setStyle(Notification.BigTextStyle().bigText(message))
                .setCategory(Notification.CATEGORY_ALARM)
                .setAutoCancel(true)
                .setContentIntent(open)
                .build()
        nm.notify(ID_BASE + item.hashCode() % ID_SPREAD, n) // the same trap again replaces its alert, different traps stack
    }

    private fun buzz() {
        val vibrator =
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                ctx.getSystemService(VibratorManager::class.java)?.defaultVibrator
            } else {
                @Suppress("DEPRECATION") // VibratorManager only exists from Android 12
                ctx.getSystemService(Vibrator::class.java)
            }
        vibrator?.vibrate(VibrationEffect.createWaveform(PATTERN, -1))
    }

    companion object {
        private const val CHANNEL = "traps"
        private const val ID_BASE = 2000
        private const val ID_SPREAD = 1000

        /** Off, on, off, on…: three strong pulses, the last the longest. */
        val PATTERN = longArrayOf(0, 700, 150, 700, 150, 1400)
    }
}
