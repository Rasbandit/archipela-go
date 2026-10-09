package dev.apgo2

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The one pending wake-up for when the next time-away mark falls due (the core works the moment out): time away needs no GPS fixes
 * or polling to finish a quest on time. Rescheduled after every refresh and presence change.
 *
 * Two triggers for the same moment: a coroutine `delay`, exact while the phone is awake, and an idle-allowed alarm, which still fires
 * when Doze has the CPU asleep (the delay would wait for the next fix or maintenance window). Whichever comes first ticks; the
 * other finds nothing due.
 */
internal class DueTimer(
    private val model: AppModel,
    private val scope: CoroutineScope,
    context: Context,
) {
    private val alarms = context.getSystemService(AlarmManager::class.java)
    private val wake =
        PendingIntent.getBroadcast(
            context,
            0,
            Intent(context, DueAlarm::class.java),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    private var job: Job? = null

    /** Replace any pending wake-up with one at the next due moment, or none when nothing will fall due. */
    fun schedule() {
        job?.cancel()
        val at = model.engine.nextDueMs(model.now())
        if (at == null) {
            alarms?.cancel(wake)
            return
        }
        alarms?.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, wake)
        job =
            scope.launch {
                delay((at - model.now()).coerceAtLeast(0))
                fire()
            }
    }

    /** The due moment came: tick the core and refresh, which reschedules the next one. */
    fun fire() {
        model.handle(model.engine.tick(model.now()))
        model.refreshPlay(withTrace = false)
    }
}

/** Receives the idle-allowed alarm (also when the process was gone: starting the app model resumes the game first). */
internal class DueAlarm : BroadcastReceiver() {
    override fun onReceive(
        context: Context,
        intent: Intent,
    ) {
        (context.applicationContext as ApgoApp).model.due.fire()
    }
}
