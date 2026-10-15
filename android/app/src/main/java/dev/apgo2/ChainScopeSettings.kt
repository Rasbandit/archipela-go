package dev.apgo2

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import uniffi.apgo_ffi.ChainScope

/** Where chains count (this zone only, or any zone): saved in the core, which decides from it what fills. */
internal class ChainScopeSettings(
    private val model: AppModel,
) {
    /** What the player picked. */
    var scope by mutableStateOf(model.engine.chainScope())
        private set

    /** Save [s]; the chips follow only once it is saved, and the bars redraw from the core. */
    fun choose(s: ChainScope) {
        val saved = runCatching { model.engine.setChainScope(s) }
        saved.onSuccess {
            scope = s
            model.refreshAll()
        }
        saved.onFailure { model.fail("set_chain_scope", "Could not save where chains count", it) }
    }
}
