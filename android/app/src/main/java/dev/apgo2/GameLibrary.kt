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
                        )
                        model.stepCal.loadInto(model.engine)
                    }
                }
            model.busy = null
            r.onSuccess {
                refreshStreets(null)
                model.sim.resetClock()
                model.log.clear()
                model.refreshAll()
                model.nav.show(AppTab.PLAY)
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

    /** Open a saved game and go to the Play tab at once; its street graph follows from [refreshStreets]. */
    fun openGame(id: String) {
        scope.launch {
            val opened =
                withContext(Dispatchers.IO) {
                    runCatching {
                        model.engine.openGame(id)
                        model.stepCal.loadInto(model.engine)
                    }
                }
            opened.onSuccess {
                Diag.info("game", "opened", "id" to id)
                refreshStreets(null)
                model.engine.logSession(true, model.now())
                model.sim.resetClock()
                model.refreshAll()
                model.nav.show(AppTab.PLAY)
            }
            opened.onFailure { model.fail("open_game", "Could not open", it) }
        }
    }

    /**
     * Build the open game's street graph (and trap-target street index) in the background and swap it in: after a game is opened or
     * started (the game plays without it meanwhile, no busy state), and after a realm it plays in ([realmId]) was edited or rescanned.
     */
    fun refreshStreets(realmId: String?) {
        scope.launch(Dispatchers.IO) {
            val r = runCatching { model.engine.refreshStreets(realmId) }
            r.onFailure { Diag.error("streets", "street graph build failed", it, "realm" to realmId) }
            r.getOrNull()?.let {
                Diag.info("streets", "street graph built", "ms" to it.buildMs, "segments" to it.segments, "degraded" to it.degraded)
            }
        }
    }

    /** On app start: reopen the game that was being played (opened and not paused) when the app stopped, on the Play tab. */
    fun resumePlaying() {
        if (!model.engine.hasGame()) model.engine.playingGame()?.let(::openGame)
    }

    /** Stop playing: log it, close the game and go to the Play tab, which then lists the saved games to continue. Tracking stops. */
    fun pause() {
        model.engine.logSession(false, model.now())
        Diag.info("game", "paused")
        model.stepCal.saveFrom(model.engine)
        model.engine.closeGame()
        model.simPos = null
        model.selected = null
        model.refreshAll()
        refreshActivity()
        model.nav.show(AppTab.PLAY)
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
