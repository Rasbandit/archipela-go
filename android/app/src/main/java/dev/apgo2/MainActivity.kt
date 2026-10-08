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
import kotlinx.coroutines.delay

private const val AP_POLL_MS = 300L

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
    val visible = rememberScreenVisible(model, perms)
    RequestPermissions(perms, model.setup.visible)
    BackgroundLocationPrompt(perms, model.setup.visible)
    TrackingEffects(model, perms, visible)
    AppRoot(model, backgroundPromptUp = perms.shouldExplainBackground(model.setup.visible))
}

// Whether the app is on screen. Also tells the model when it leaves and returns, and re-reads the permissions that are granted on a
// system settings page.
@Composable
private fun rememberScreenVisible(
    model: AppModel,
    perms: PermissionState,
): Boolean {
    val owner = LocalLifecycleOwner.current
    var visible by remember { mutableStateOf(true) }
    DisposableEffect(owner) {
        val observer =
            LifecycleEventObserver { _, event ->
                when (event) {
                    Lifecycle.Event.ON_START -> {
                        visible = true
                        perms.refresh()
                        model.onForeground()
                    }

                    Lifecycle.Event.ON_STOP -> {
                        visible = false
                        model.onBackground()
                    }

                    else -> {}
                }
            }
        owner.lifecycle.addObserver(observer)
        onDispose { owner.lifecycle.removeObserver(observer) }
    }
    return visible
}

// Starts and stops what runs while the game is played: step counter, location, Wi-Fi/Bluetooth monitor, the tracking service and
// the Archipelago connection.
@Composable
private fun TrackingEffects(
    model: AppModel,
    perms: PermissionState,
    visible: Boolean,
) {
    val context = LocalContext.current
    LaunchedEffect(perms.steps) { if (perms.steps) model.sensors.startSteps() }
    // The presence decision picks the rate: precise in a zone, coarse outside, off at home or in the car. Stopped
    // (no game) keeps the map marker while the app is on screen.
    LaunchedEffect(perms.location, model.hud != null, visible, model.presence.decision) {
        model.presence.appVisible = visible
        model.presence.locationPermitted = perms.location
        if (perms.location) model.presence.applyLocation() else model.sensors.stopLocation()
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
    LaunchedEffect(model.ap.session) {
        while (model.ap.session != null) {
            model.ap.tick()
            delay(AP_POLL_MS)
        }
    }
}
