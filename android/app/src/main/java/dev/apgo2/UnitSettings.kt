package dev.apgo2

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.apgo2.ui.Units
import uniffi.apgo_ffi.UnitChoice
import java.util.Locale

/** The distance-unit setting: saved in the core, which also resolves Auto from the phone's region into [Units.system]. */
internal class UnitSettings(
    private val model: AppModel,
) {
    /** What the player picked. */
    var choice by mutableStateOf(model.engine.unitChoice())
        private set

    init {
        apply()
    }

    /** Save [c] and redraw every distance in it. */
    fun choose(c: UnitChoice) {
        val saved = runCatching { model.engine.setUnitChoice(c) }
        saved.onSuccess {
            choice = c
            apply()
        }
        saved.onFailure { model.fail("set_unit_choice", "Could not save the units", it) }
    }

    private fun apply() {
        Units.system = model.engine.units(Locale.getDefault().country)
    }
}
