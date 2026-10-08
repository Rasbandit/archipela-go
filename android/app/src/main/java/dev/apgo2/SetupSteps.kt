package dev.apgo2

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothManager
import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.presence.CarChoices
import dev.apgo2.presence.CarDevice
import dev.apgo2.presence.HomeNetwork
import dev.apgo2.presence.PresenceSignals
import dev.apgo2.presence.WifiChoice
import dev.apgo2.presence.WifiChoices
import dev.apgo2.presence.WifiScanner
import dev.apgo2.ui.FeedbackText
import dev.apgo2.ui.SetupText
import dev.apgo2.ui.Tone
import kotlinx.coroutines.delay

/**
 * The frame every text step shares: title, why it matters, a fixed [header] (search), a list that scrolls in the space left over,
 * a fixed [footer]
 * (always just above the buttons and the keyboard), and Back / Skip / Next.
 */
@Composable
internal fun StepPage(
    title: String,
    why: String,
    next: String,
    skip: String?,
    onBack: () -> Unit,
    onNext: () -> Unit,
    header: @Composable ColumnScope.() -> Unit = {},
    footer: @Composable ColumnScope.() -> Unit = {},
    content: @Composable ColumnScope.() -> Unit,
) {
    BackHandler { onBack() }
    Column(
        Modifier
            .fillMaxSize()
            .statusBarsPadding()
            .navigationBarsPadding()
            .imePadding()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(title, style = MaterialTheme.typography.titleLarge)
        Text(why, style = MaterialTheme.typography.bodyMedium)
        header()
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            content = content,
        )
        footer()
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onBack) { Text("Back") }
            Spacer(Modifier.weight(1f))
            skip?.let { TextButton(onClick = onNext) { Text(it) } }
            Button(onClick = onNext) { Text(next) }
        }
    }
}

private const val SCAN_WAIT_MS = 3_000L

// What the Wi-Fi step shows and does; it lives as long as the step is on screen.
@Stable
private class WifiStepState(
    private val m: AppModel,
    private val scanner: WifiScanner,
) {
    var saved by mutableStateOf(m.settings.homeNetworks)
    var nearby by mutableStateOf(scanner.nearby())
    var query by mutableStateOf("")
    var typed by mutableStateOf("")

    // Names unticked during this visit stay listed (unticked) until the step closes, so a slip can be undone.
    var removed by mutableStateOf(emptyList<String>())
    var note by mutableStateOf<String?>(null)
    var scanning by mutableStateOf(false)

    fun choices() = WifiChoices.merge(saved, m.presence.monitor.currentNetwork(), nearby + removed, query)

    fun inRange() = nearby.mapNotNull { PresenceSignals.cleanSsid(it) }.toSet()

    // Location was just allowed: the last results are readable now.
    fun readLastResults() {
        nearby = scanner.nearby()
        if (scanner.rescan()) scanning = true
    }

    // The scan answers a moment later; read it then.
    suspend fun readScanWhenDone() {
        delay(SCAN_WAIT_MS)
        nearby = scanner.nearby()
        scanning = false
    }

    fun rescan() {
        if (scanner.rescan()) {
            scanning = true
            note = null
        } else {
            note = "Android limits how often Wi-Fi can be scanned. Showing the last results."
        }
    }

    fun toggle(
        c: WifiChoice,
        on: Boolean,
    ) {
        if (on) {
            m.settings.addHome(HomeNetwork(c.ssid, c.bssid))
        } else {
            m.settings.removeHome(c.ssid)
            removed = removed + c.ssid
        }
        saved = m.settings.homeNetworks
        m.presence.evaluate()
    }

    fun addTyped() {
        val ssid = PresenceSignals.cleanSsid(typed) ?: return
        m.settings.addHome(HomeNetwork(ssid, null))
        saved = m.settings.homeNetworks
        removed = removed - ssid
        typed = ""
        query = ""
        m.presence.evaluate()
    }

    fun tagFor(c: WifiChoice) =
        when {
            c.connected -> "Connected now"
            c.saved && nearby.isNotEmpty() && c.ssid !in inRange() -> "Saved, not in range"
            else -> null
        }
}

/** Step 2: tick every Wi-Fi network your home uses. Saving a name covers every access point on it. */
@Composable
internal fun WifiStep(
    m: AppModel,
    onBack: () -> Unit,
    onNext: () -> Unit,
) {
    val ctx = LocalContext.current
    val state = remember { WifiStepState(m, WifiScanner(ctx)) }
    LaunchedEffect(state.scanning) { if (state.scanning) state.readScanWhenDone() }
    LaunchedEffect(m.presence.locationPermitted) { if (m.presence.locationPermitted) state.readLastResults() }
    StepPage(
        title = "Home Wi-Fi · step 2 of 3",
        why = SetupText.WIFI_WHY,
        next = "Next",
        skip = if (state.saved.isEmpty()) "Skip, I'll do this at home" else null,
        onBack = onBack,
        onNext = onNext,
        header = {
            WifiSearch(state)
            if (!m.presence.locationPermitted) FeedbackText(SetupText.WIFI_NEEDS_LOCATION, Tone.Warning)
            state.note?.let { Text(it, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        },
        footer = { WifiAdd(state) },
    ) {
        WifiChoiceList(state)
    }
}

@Composable
private fun WifiSearch(state: WifiStepState) {
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        OutlinedTextField(state.query, { state.query = it }, Modifier.weight(1f), label = { Text("Search networks") }, singleLine = true)
        OutlinedButton(enabled = !state.scanning, onClick = state::rescan) { Text(if (state.scanning) "Scanning…" else "Rescan") }
    }
}

@Composable
private fun WifiAdd(state: WifiStepState) {
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        OutlinedTextField(
            state.typed,
            { state.typed = it },
            Modifier.weight(1f),
            label = { Text("Add a network by name") },
            singleLine = true,
        )
        OutlinedButton(enabled = PresenceSignals.cleanSsid(state.typed) != null, onClick = state::addTyped) { Text("Add") }
    }
}

@Composable
private fun WifiChoiceList(state: WifiStepState) {
    val choices = state.choices()
    if (choices.isEmpty()) {
        Text(
            if (state.query.isBlank()) SetupText.WIFI_NONE_FOUND else "Nothing matches \"${state.query}\".",
            fontSize = 12.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
    choices.forEach { c ->
        Row(Modifier.fillMaxWidth().clickable { state.toggle(c, !c.saved) }, verticalAlignment = Alignment.CenterVertically) {
            Checkbox(c.saved, { state.toggle(c, it) })
            Column {
                Text(c.ssid)
                state.tagFor(c)?.let { Text(it, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
        }
    }
}

/** Step 3: tick the Bluetooth device that is your car. Optional. */
@Composable
@SuppressLint("InlinedApi") // only asked when hasBluetoothConnect() is false, which cannot happen before Android 12
internal fun CarStep(
    m: AppModel,
    onBack: () -> Unit,
    onDone: () -> Unit,
) {
    val ctx = LocalContext.current
    var car by remember { mutableStateOf(m.settings.carDevices) }
    var query by remember { mutableStateOf("") }
    // Tell the model too: it restarts the monitor so car detection works without leaving the app. A grant made on the settings page
    // is picked up when the step resumes (and by the activity's own re-read on start).
    val askBt = rememberPermissionAsk(Manifest.permission.BLUETOOTH_CONNECT, ctx::hasBluetoothConnect, m.presence::ensureMonitor)
    val btOk = askBt.action == PermissionAsk.Granted
    val paired = remember(btOk) { if (btOk) pairedDevices(ctx) else emptyList() }

    fun toggle(
        d: CarDevice,
        on: Boolean,
    ) {
        val now = m.settings.carDevices.filterNot { it.address == d.address }
        m.settings.setCar(if (on) now + d else now)
        car = m.settings.carDevices
        m.presence.evaluate()
    }
    StepPage(
        title = "Car Bluetooth · step 3 of 3",
        why = SetupText.CAR_WHY,
        next = "Finish",
        skip = if (car.isEmpty()) "Skip" else null,
        onBack = onBack,
        onNext = onDone,
        header = {
            OutlinedTextField(
                query,
                { query = it },
                Modifier.fillMaxWidth(),
                label = { Text("Search devices") },
                singleLine = true,
            )
        },
    ) {
        if (!btOk) {
            AllowBluetooth(askBt)
        } else if (paired.isEmpty() && car.isEmpty()) {
            Text(SetupText.CAR_NONE_PAIRED, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        CarChoices.merge(paired, car, query).forEach { d ->
            val on = car.any { it.address == d.address }
            Row(Modifier.fillMaxWidth().clickable { toggle(d, !on) }, verticalAlignment = Alignment.CenterVertically) {
                Checkbox(on, { toggle(d, it) })
                Text(d.name)
            }
        }
    }
}

// The Allow button, or a link to the app's settings once Android no longer shows its dialog.
@Composable
private fun AllowBluetooth(ask: PermissionAskState) {
    val blocked = ask.action == PermissionAsk.OpenSettings
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedButton(onClick = ask::ask) { Text(if (blocked) "Open settings to allow Bluetooth" else "Allow Bluetooth to pick your car") }
        Text(
            if (blocked) SetupText.CAR_BLUETOOTH_BLOCKED else SetupText.CAR_NEEDS_BLUETOOTH,
            fontSize = 12.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

// The phone's paired Bluetooth devices; empty when the adapter is missing or the system refuses.
// The caller only offers this once Bluetooth is allowed, and runCatching absorbs a SecurityException if it is revoked meanwhile.
@SuppressLint("MissingPermission")
private fun pairedDevices(ctx: Context): List<CarDevice> =
    runCatching {
        ctx
            .getSystemService(BluetoothManager::class.java)
            ?.adapter
            ?.bondedDevices
            ?.map { CarDevice(it.name ?: it.address, it.address) }
            ?: emptyList()
    }.getOrDefault(emptyList())
