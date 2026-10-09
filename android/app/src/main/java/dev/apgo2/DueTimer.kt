package dev.apgo2

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The one pending wake-up for when the next time-away mark falls due (the core works the moment out): time away needs no GPS fixes
 * or polling to finish a quest on time. Rescheduled after every refresh and presence change.
 */
internal class DueTimer(
    private val model: AppModel,
    private val scope: CoroutineScope,
) {
    private var job: Job? = null

    /** Replace any pending wake-up with one at the next due moment, or none when nothing will fall due. */
    fun schedule() {
        job?.cancel()
        val at = model.engine.nextDueMs(model.now()) ?: return
        job =
            scope.launch {
                delay((at - model.now()).coerceAtLeast(0))
                model.handle(model.engine.tick(model.now()))
                model.refreshPlay(withTrace = false) // reschedules the next one
            }
    }
}
