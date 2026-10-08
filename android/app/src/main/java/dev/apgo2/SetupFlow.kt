package dev.apgo2

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import dev.apgo2.presence.SetupStep
import dev.apgo2.ui.SetupText

/** First-run and edit wizard for Home Base: where home is, which Wi-Fi networks are home, which Bluetooth device is the car. */
@Composable
fun SetupFlow(m: AppModel) {
    var step by rememberSaveable { mutableStateOf(m.setupStart) }
    when (step) {
        SetupStep.Home -> HomePicker(m, title = "${SetupText.homeBaseName} · step 1 of 3", onBack = { m.leaveSetup() }, onNext = { step = SetupStep.Wifi })
        SetupStep.Wifi -> WifiStep(m, onBack = { step = SetupStep.Home }, onNext = { step = SetupStep.Car })
        SetupStep.Car -> CarStep(m, onBack = { step = SetupStep.Wifi }, onDone = { m.finishSetup() })
    }
}
