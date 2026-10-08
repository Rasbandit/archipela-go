package dev.apgo2

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.apgo_ffi.SoloOptionsIn
import java.util.UUID
import kotlin.random.Random

private const val ACTIVITY_LIMIT = 300u

/** Starting, opening, pausing and deleting saved games, and the things done to the open game from its screens. */
internal class GameLibrary(
    private val model: AppModel,
    private val scope: CoroutineScope,
) {
    /** Build and open a new solo game over the chosen realms. */
    fun startSolo(
        opts: SoloOptionsIn,
        zoneRealms: List<String>,
        name: String,
        awayZoneOnly: Boolean,
        awayDistanceM: UInt,
    ) {
        scope.launch {
            model.busy = "Building your game..."
            val seed = Random.nextLong().toULong() shr 1
            val r =
                withContext(Dispatchers.IO) {
                    runCatching {
                        model.engine.startSolo(
                            UUID.randomUUID().toString(),
                            name,
                            opts,
                            zoneRealms,
                            seed,
                            model.surfacePref,
                            model.avoidStairs,
                            awayZoneOnly,
                            awayDistanceM,
                        )
                    }
                }
            model.busy = null
            r.onSuccess {
                model.sim.resetClock()
                model.log.clear()
                model.refreshAll()
                model.tab = AppTab.PLAY
                model.status = "Game started!"
            }
            r.onFailure { model.fail("start_game", "Could not start", it) }
        }
    }

    /** Build the YAML for an Archipelago multiworld from these options and show it. */
    fun exportYaml(opts: SoloOptionsIn) {
        val yaml = runCatching { model.engine.buildYaml("Player", opts) }
        yaml.onSuccess { model.yamlText = it }
        yaml.onFailure { model.fail("yaml", "YAML failed", it) }
    }

    /** Open a saved game and go to the Play tab. */
    fun openGame(id: String) {
        val opened = runCatching { model.engine.openGame(id) }
        opened.onSuccess {
            Diag.info("game", "opened", "id" to id)
            model.engine.logSession(true, model.now())
            model.sim.resetClock()
            model.refreshAll()
            model.tab = AppTab.PLAY
        }
        opened.onFailure { model.fail("open_game", "Could not open", it) }
    }

    /** Stop playing: log it, close the game and go to the Play tab, which then lists the saved games to continue. Tracking stops. */
    fun pause() {
        model.engine.logSession(false, model.now())
        Diag.info("game", "paused")
        model.engine.closeGame()
        model.simPos = null
        model.selected = null
        model.refreshAll()
        refreshActivity()
        model.tab = AppTab.PLAY
    }

    /** Give an unfinished quest a new place (the player's own reroll, not the Shuffle trap). */
    fun reroll(id: Long) {
        runCatching { model.engine.reroll(listOf(id), (Random.nextLong() ushr 1).toULong()) }.onFailure { Diag.failure("reroll", it) }
        model.refreshPlay()
    }

    /** Reload what happened in the open (or last paused) game. */
    fun refreshActivity() {
        model.activity = model.engine.activity(ACTIVITY_LIMIT)
    }

    /** Delete a saved game. */
    fun deleteGame(id: String) {
        runCatching { model.engine.deleteGame(id) }
        model.refreshAll()
    }
}
