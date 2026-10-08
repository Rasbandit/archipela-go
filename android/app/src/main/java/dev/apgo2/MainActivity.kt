package dev.apgo2

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.core.content.ContextCompat
import androidx.compose.material3.Surface
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import dev.apgo2.ui.ApgoTheme
import kotlinx.coroutines.delay

class MainActivity : ComponentActivity() {
    private fun hasBackgroundLocation() =
        Build.VERSION.SDK_INT < 29 || ContextCompat.checkSelfPermission(this, Manifest.permission.ACCESS_BACKGROUND_LOCATION) == PackageManager.PERMISSION_GRANTED

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Transparent system bars, with dark or light icons chosen from the system theme (the app theme follows the same setting, so they agree).
        enableEdgeToEdge()
        // The model lives in the Application: sensors and the game keep going if this activity is recreated or destroyed.
        val model = (application as ApgoApp).model
        setContent {
            ApgoTheme {
                // No manual status-bar padding: Scaffold insets its own content, and this surface paints behind the bars.
                Surface(Modifier.fillMaxSize()) {
                    val owner = LocalLifecycleOwner.current
                    // "Allow all the time". Re-read on every start: the user grants it on a system settings page, not in a dialog.
                    var bgGranted by remember { mutableStateOf(hasBackgroundLocation()) }
                    DisposableEffect(owner) {
                        val obs = LifecycleEventObserver { _, e ->
                            when (e) {
                                Lifecycle.Event.ON_START -> { bgGranted = hasBackgroundLocation(); model.onForeground() }
                                Lifecycle.Event.ON_STOP -> model.onBackground()
                                else -> {}
                            }
                        }
                        owner.lifecycle.addObserver(obs)
                        onDispose { owner.lifecycle.removeObserver(obs) }
                    }

                    // Permissions, one after another: location, then step counter, then notifications (the tracking notification).
                    var permitted by remember { mutableStateOf(false) }
                    val askNotifications = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { Diag.i("permission", "notifications", "granted" to it) }
                    var stepsOk by remember { mutableStateOf(false) }
                    val askSteps = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
                        stepsOk = it
        Diag.i("permission", "activity_recognition", "granted" to it)
                        if (Build.VERSION.SDK_INT >= 33) askNotifications.launch(Manifest.permission.POST_NOTIFICATIONS)
                    }
                    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { permitted = it; Diag.i("permission", "fine_location", "granted" to it) }
                    LaunchedEffect(Unit) { ask.launch(Manifest.permission.ACCESS_FINE_LOCATION) }
                    LaunchedEffect(permitted) { if (permitted) askSteps.launch(Manifest.permission.ACTIVITY_RECOGNITION) }

                    val askBackground = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
                        bgGranted = it || hasBackgroundLocation()
                        Diag.i("permission", "background_location", "granted" to bgGranted)
                    }
                    var bgDeclined by remember { mutableStateOf(getSharedPreferences("prefs", MODE_PRIVATE).getBoolean("bg_declined", false)) }
                    if (permitted && !bgGranted && !bgDeclined && Build.VERSION.SDK_INT >= 29) {
                        AlertDialog(
                            onDismissRequest = {},
                            title = { Text("Track with the screen off") },
                            text = { Text("To keep recording your route and completing quests while the phone is in your pocket, choose \"Allow all the time\" for location on the next screen. Your location stays on this phone.") },
                            confirmButton = { TextButton(onClick = { askBackground.launch(Manifest.permission.ACCESS_BACKGROUND_LOCATION) }) { Text("Continue") } },
                            dismissButton = {
                                TextButton(onClick = {
                                    bgDeclined = true
                                    getSharedPreferences("prefs", MODE_PRIVATE).edit().putBoolean("bg_declined", true).apply()
                                    Diag.i("permission", "background_location", "granted" to false, "declined_in_app" to true)
                                }) { Text("Not now") }
                            },
                        )
                    }

                    LaunchedEffect(stepsOk) { if (stepsOk) model.sensors.startSteps() }
                    val rate = GpsPolicy.forState(playing = model.quests.isNotEmpty())
                    LaunchedEffect(permitted, rate) { if (permitted) model.sensors.startLocation(rate) }
                    // A game that is open is tracked in the foreground service, so fixes keep coming with the screen off.
                    val playing = model.hud != null
                    LaunchedEffect(permitted, playing) {
                        if (permitted && playing) runCatching { TrackingService.start(applicationContext) } else TrackingService.stop(applicationContext)
                    }
                    LaunchedEffect(model.session) { while (model.session != null) { model.apTick(); delay(300) } }
                    AppRoot(model)
                }
            }
        }
    }
}
