package dev.apgo2

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat

private const val PREFS = "prefs"
private const val BG_DECLINED = "bg_declined"
private const val LOG_TAG = "permission"
private const val GRANTED = "granted"
private const val BACKGROUND_LOCATION = "background_location"

private const val BACKGROUND_PROMPT =
    """To keep recording your route and completing quests while the phone is in your pocket, choose "Allow all the time" """
private const val BACKGROUND_PROMPT_TAIL = "for location on the next screen. Your location stays on this phone."

internal fun Context.hasPermission(permission: String) =
    ContextCompat.checkSelfPermission(this, permission) == PackageManager.PERMISSION_GRANTED

/** True when the app may read Bluetooth connections (always before Android 12, which introduced the permission). */
internal fun Context.hasBluetoothConnect() =
    Build.VERSION.SDK_INT < Build.VERSION_CODES.S || hasPermission(Manifest.permission.BLUETOOTH_CONNECT)

/** What the player has allowed so far; the values change as the prompts are answered or the system settings page is used. */
@Stable
internal class PermissionState(
    private val ctx: Context,
) {
    // Start from the real state: on an activity recreate "false" would stop tracking until the launcher answers.
    var location by mutableStateOf(ctx.hasPermission(Manifest.permission.ACCESS_FINE_LOCATION))
    var steps by mutableStateOf(ctx.hasPermission(Manifest.permission.ACTIVITY_RECOGNITION))

    // "Allow all the time". Re-read on every start: the user grants it on a system settings page, not in a dialog.
    var background by mutableStateOf(hasBackgroundLocation())
    var bluetooth by mutableStateOf(ctx.hasBluetoothConnect())
    var backgroundDeclined by mutableStateOf(prefs().getBoolean(BG_DECLINED, false))

    /** Read the permissions that are granted outside of a prompt again. */
    fun refresh() {
        background = hasBackgroundLocation()
        bluetooth = ctx.hasBluetoothConnect()
    }

    /** True when the explanation for "Allow all the time" should be shown. */
    fun shouldExplainBackground() = location && !background && !backgroundDeclined && Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q

    /** The system answered the "Allow all the time" request ([granted]); the settings page may have granted it as well. */
    fun onBackgroundAnswer(granted: Boolean) {
        background = granted || hasBackgroundLocation()
        Diag.info(LOG_TAG, BACKGROUND_LOCATION, GRANTED to background)
    }

    /** Remember that the player said "Not now" to the background prompt. */
    fun declineBackground() {
        backgroundDeclined = true
        prefs().edit().putBoolean(BG_DECLINED, true).apply()
        Diag.info(LOG_TAG, BACKGROUND_LOCATION, GRANTED to false, "declined_in_app" to true)
    }

    private fun prefs() = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    private fun hasBackgroundLocation() =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.Q || ctx.hasPermission(Manifest.permission.ACCESS_BACKGROUND_LOCATION)
}

/** Remember the permission state of this app for as long as the composition lives. */
@Composable
internal fun rememberPermissionState(): PermissionState {
    val ctx = LocalContext.current
    return remember { PermissionState(ctx) }
}

/** Asks for the permissions one after another: location, then step counter, then notifications (the tracking notification). */
@Composable
internal fun RequestPermissions(perms: PermissionState) {
    val askNotifications =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
            Diag.info(LOG_TAG, "notifications", GRANTED to it)
        }
    val askSteps =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
            perms.steps = it
            Diag.info(LOG_TAG, "activity_recognition", GRANTED to it)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) askNotifications.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    val askLocation =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
            perms.location = it
            Diag.info(LOG_TAG, "fine_location", GRANTED to it)
        }
    LaunchedEffect(Unit) { askLocation.launch(Manifest.permission.ACCESS_FINE_LOCATION) }
    LaunchedEffect(perms.location) { if (perms.location) askSteps.launch(Manifest.permission.ACTIVITY_RECOGNITION) }
}

/** Explains why "Allow all the time" is wanted, once location is allowed, until the player accepts or declines. */
@Composable
internal fun BackgroundLocationPrompt(perms: PermissionState) {
    val askBackground =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission(), perms::onBackgroundAnswer)
    if (!perms.shouldExplainBackground()) return
    AlertDialog(
        onDismissRequest = {},
        title = { Text("Track with the screen off") },
        text = { Text(BACKGROUND_PROMPT + BACKGROUND_PROMPT_TAIL) },
        confirmButton = {
            TextButton(onClick = { askBackground.launch(Manifest.permission.ACCESS_BACKGROUND_LOCATION) }) { Text("Continue") }
        },
        dismissButton = { TextButton(onClick = perms::declineBackground) { Text("Not now") } },
    )
}
