package dev.apgo2

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** The trap alerts switch: saved in the core, which decides from it whether a trap alerts (#160). */
internal class TrapSettings(
    private val model: AppModel,
) {
    /** Whether a trap alerts the player. */
    var on by mutableStateOf(model.engine.trapAlerts())
        private set

    /** Save [value]; the switch follows only once it is saved. */
    fun set(value: Boolean) {
        val saved = runCatching { model.engine.setTrapAlerts(value) }
        saved.onSuccess { on = value }
        saved.onFailure { model.fail("set_trap_alerts", "Could not save trap alerts", it) }
    }
}
