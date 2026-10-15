package dev.apgo2

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import dev.apgo2.presence.SetupStep
import dev.apgo2.ui.CampText

/** First-run and edit wizard: where your first camp is, which Wi-Fi networks are camps, which Bluetooth device is the car. */
@Composable
internal fun SetupFlow(m: AppModel) {
    val ctx = LocalContext.current
    var step by rememberSaveable { mutableStateOf(m.setup.startStep) }
    when (step) {
        SetupStep.Home -> {
            CampPicker(
                m,
                title = CampText.FIRST_STEP,
                campId = m.camps.firstOrNull()?.id,
                askName = false,
                onBack = { m.setup.leave() },
                onNext = { step = SetupStep.Wifi },
            )
        }

        SetupStep.Wifi -> {
            WifiStep(m, m.setup.wifiCamp, onBack = { step = SetupStep.Home }, onNext = { step = SetupStep.Car })
        }

        SetupStep.Car -> {
            CarStep(m, onBack = { step = SetupStep.Wifi }, onDone = {
                if (BatteryGuide.needed(ctx.ignoringBatteryOptimizations())) step = SetupStep.Battery else m.setup.finish()
            })
        }

        SetupStep.Battery -> {
            BatteryStep(onBack = { step = SetupStep.Car }, onDone = { m.setup.finish() })
        }
    }
}
