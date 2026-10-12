package dev.apgo2

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import dev.apgo2.ui.ApgoTheme

/** The single activity: hosts the Compose UI and handles the location and notification permission prompts. */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Transparent system bars, with dark or light icons chosen from the system theme (the app theme follows the same setting,
        // so they agree).
        enableEdgeToEdge()
        // The model lives in the Application: sensors and the game keep going if this activity is recreated or destroyed.
        val model = (application as ApgoApp).model
        setContent {
            ApgoTheme {
                // No manual status-bar padding: Scaffold insets its own content, and this surface paints behind the bars.
                Surface(Modifier.fillMaxSize()) { MainContent(model) }
            }
        }
    }
}

@Composable
private fun MainContent(model: AppModel) {
    val perms = rememberPermissionState()
    LifecycleHooks(model, perms)
    RequestPermissions(perms, model.setup.visible)
    BackgroundLocationPrompt(perms, model.setup.visible)
    TrackingEffects(model, perms)
    AppRoot(model, backgroundPromptUp = perms.shouldExplainBackground(model.setup.visible))
}

// Tells the model when the app leaves the screen and returns, and re-reads the permissions that are granted on a system settings page.
@Composable
private fun LifecycleHooks(
    model: AppModel,
    perms: PermissionState,
) {
    val owner = LocalLifecycleOwner.current
    DisposableEffect(owner) {
        val observer =
            LifecycleEventObserver { _, event ->
                when (event) {
                    Lifecycle.Event.ON_START -> {
                        perms.refresh()
                        model.onForeground()
                    }

                    Lifecycle.Event.ON_STOP -> {
                        model.onBackground()
                    }

                    else -> {}
                }
            }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer) }
    }
}

// Starts and stops what runs while the game is played: step counter, location, Wi-Fi/Bluetooth monitor, the tracking service and
// the Archipelago connection.
@Composable
private fun TrackingEffects(
    model: AppModel,
    perms: PermissionState,
) {
    val context = LocalContext.current
    LaunchedEffect(perms.steps) { if (perms.steps) model.sensors.allowSteps() }
    // The presence decision picks the rate: precise in a zone, coarse outside, off at home or in the car. Stopped
    // (no game) keeps the map marker while the app is on screen.
    // Visibility is applied by the lifecycle (AppModel.onForeground/onBackground), which runs while the activity is stopped too.
    LaunchedEffect(perms.location, model.hud != null, model.presence.decision) {
        model.presence.locationPermitted = perms.location
        model.presence.applyLocation() // with location not permitted it stops location and still sets the step counter
    }
    // Wi-Fi names need location permission; Bluetooth devices need BLUETOOTH_CONNECT (re-read on every start, restarting the
    // monitor when it appears). The model owns the monitor for the whole process; this only tells it when it may start or the
    // Bluetooth grant changed.
    LaunchedEffect(perms.location, perms.bluetooth) { if (perms.location) model.presence.ensureMonitor(perms.bluetooth) }
    // A game that is open is tracked in the foreground service, so fixes keep coming with the screen off.
    val playing = model.hud != null
    LaunchedEffect(perms.location, playing) {
        if (perms.location && playing) {
            runCatching { TrackingService.start(context.applicationContext) }
        } else {
            TrackingService.stop(context.applicationContext)
        }
    }
}
