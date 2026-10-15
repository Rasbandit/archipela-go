package dev.apgo2

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.apgo2.presence.SetupProgress
import dev.apgo2.presence.SetupStep

/** The setup wizard (first camp, camp Wi-Fi, car Bluetooth). */
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

    /** The camp the Wi-Fi step opens on (a camp row's Wi-Fi button), or null to let the step pick. */
    var wifiCamp: String? = null
        private set

    /** How far along the wizard the player is, for the camps card. */
    fun progress() =
        SetupProgress(
            model.camps.isNotEmpty(),
            model.settings.homeNetworks.size,
            model.settings.carDevices.size,
            model.settings.setupDone,
        )

    /** Open the wizard on [from] (the first step by default), with the Wi-Fi step on camp [camp] when given. */
    fun open(
        from: SetupStep? = null,
        camp: String? = null,
    ) {
        startStep = from ?: SetupStep.Home
        wifiCamp = camp
        visible = true
    }

    /** Close without marking it done (Back on the first step): the camps card keeps offering it. */
    fun leave() {
        visible = false
        model.presence.evaluate() // a home Wi-Fi saved in the wizard withdraws a pending "add this Wi-Fi?" offer
    }

    /** Mark the wizard done and close it. */
    fun finish() {
        model.settings.setupDone = true
        visible = false
        model.presence.evaluate()
    }
}
