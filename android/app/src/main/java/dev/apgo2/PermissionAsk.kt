package dev.apgo2

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.LocalActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.core.app.ActivityCompat
import androidx.core.content.edit
import androidx.lifecycle.compose.LifecycleResumeEffect

private const val PREFS = "prefs"
private const val DENIED_PREFIX = "denied:"
private const val LOG_TAG = "permission"

/** What an in-screen "allow" button should do for one runtime permission. */
internal enum class PermissionAsk { Granted, Request, OpenSettings }

/**
 * Picks the button for a permission. Android stops showing its dialog once the player has said no for good, and it only tells
 * us so indirectly: the last answer was a denial ([deniedBefore]) and it no longer wants a rationale shown ([showRationale]).
 * Before the first request the rationale flag is false as well, which is why the earlier denial has to be remembered.
 */
internal fun permissionAsk(
    granted: Boolean,
    deniedBefore: Boolean,
    showRationale: Boolean,
): PermissionAsk =
    when {
        granted -> PermissionAsk.Granted
        deniedBefore && !showRationale -> PermissionAsk.OpenSettings
        else -> PermissionAsk.Request
    }

/** One permission asked from a screen button: [action] says which button to show, [ask] does it. */
@Stable
internal class PermissionAskState(
    private val ctx: Context,
    private val activity: Activity?,
    private val permission: String,
    private val granted: () -> Boolean,
) {
    var action by mutableStateOf(read())
        private set

    internal var launchRequest: () -> Unit = {}

    /** Read the grant again (the settings page may have changed it while the app was away). */
    fun refresh() {
        action = read()
    }

    /** The system dialog answered: remember a denial so a later "never ask again" can be told apart from "never asked". */
    fun onAnswer(isGranted: Boolean) {
        prefs().edit { putBoolean(DENIED_PREFIX + permission, !isGranted) }
        Diag.info(LOG_TAG, permission, "granted" to isGranted)
        refresh()
    }

    /** Show the system dialog, or the app's settings page when Android will no longer show it. */
    fun ask() {
        when (action) {
            PermissionAsk.Granted -> Unit
            PermissionAsk.Request -> launchRequest()
            PermissionAsk.OpenSettings -> openAppSettings()
        }
    }

    private fun openAppSettings() {
        Diag.info(LOG_TAG, permission, "open_settings" to true)
        val intent =
            Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", ctx.packageName, null))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        runCatching { ctx.startActivity(intent) }.onFailure { Diag.error(LOG_TAG, "app settings did not open", it) }
    }

    private fun read() =
        permissionAsk(
            granted = granted(),
            deniedBefore = prefs().getBoolean(DENIED_PREFIX + permission, false),
            showRationale = activity?.let { ActivityCompat.shouldShowRequestPermissionRationale(it, permission) } ?: false,
        )

    private fun prefs() = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
}

/**
 * Remembers a [PermissionAskState] for [permission], re-read each time the screen resumes (back from the settings page).
 * [granted] reads the current grant; [onAnswer] hears every answer of the system dialog.
 */
@Composable
internal fun rememberPermissionAsk(
    permission: String,
    granted: () -> Boolean,
    onAnswer: (Boolean) -> Unit = {},
): PermissionAskState {
    val ctx = LocalContext.current
    val activity = LocalActivity.current
    val answer by rememberUpdatedState(onAnswer)
    val state = remember(permission) { PermissionAskState(ctx, activity, permission, granted) }
    val launcher =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
            state.onAnswer(it)
            answer(it)
        }
    state.launchRequest = { launcher.launch(permission) }
    LifecycleResumeEffect(state) {
        state.refresh()
        onPauseOrDispose {}
    }
    return state
}
