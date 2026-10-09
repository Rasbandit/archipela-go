package dev.apgo2

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.apgo2.ui.Units
import uniffi.apgo_ffi.UnitChoice
import java.util.Locale

/** The distance-unit setting: saved in the core, which resolves Auto from the phone's region and writes its own text in it. */
internal class UnitSettings(
    private val model: AppModel,
) {
    /** What the player picked. */
    var choice by mutableStateOf(model.engine.unitChoice())
        private set

    init {
        follow() // the rest of the model is not built yet, so nothing to redraw
    }

    /** Save [c] and redraw every distance in it, including the quest and goal text the core writes. */
    fun choose(c: UnitChoice) {
        val saved = runCatching { model.engine.setUnitChoice(c) }
        saved.onSuccess {
            choice = c
            follow()
            model.refreshAll()
        }
        saved.onFailure { model.fail("set_unit_choice", "Could not save the units", it) }
    }

    /** Back in the foreground: the phone's region may have changed meanwhile (Auto follows it), so redraw if the units moved. */
    fun onForeground() {
        if (follow()) model.refreshAll()
    }

    // Pass the region to the core and take the units it resolves; true when they changed.
    private fun follow(): Boolean {
        model.engine.setRegion(Locale.getDefault().country)
        val now = model.engine.units()
        val changed = now != Units.system
        Units.system = now
        return changed
    }
}
