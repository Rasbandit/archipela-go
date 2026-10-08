package dev.apgo2

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.apgo2.presence.SetupProgress
import dev.apgo2.presence.SetupStep

/** The setup wizard (home pin, home Wi-Fi, car Bluetooth). */
internal class SetupWizard(
    private val model: AppModel,
) {
    /**
     * The wizard is open. It opens by itself on each app (process) start until it has been finished or skipped once.
     */
    var visible by mutableStateOf(!model.settings.setupDone)
        private set

    /** The step the wizard opens on. */
    var startStep = SetupStep.Home
        private set

    /** How far along the wizard the player is, for the Home card. */
    fun progress() =
        SetupProgress(
            model.home != null,
            model.settings.homeNetworks.size,
            model.settings.carDevices.size,
            model.settings.setupDone,
        )

    /** Open the wizard on [from] (the first step by default). */
    fun open(from: SetupStep? = null) {
        startStep = from ?: SetupStep.Home
        visible = true
    }

    /** Close without marking it done (Back on the first step): the Home card keeps offering it. */
    fun leave() {
        visible = false
    }

    /** Mark the wizard done and close it. */
    fun finish() {
        model.settings.setupDone = true
        visible = false
    }
}
