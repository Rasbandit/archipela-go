package dev.apgo2

import android.Manifest
import android.bluetooth.BluetoothManager
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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

/** Step 2: tick every Wi-Fi network your home uses. Saving a name covers every access point on it. */
@Composable
internal fun WifiStep(
    m: AppModel,
    onBack: () -> Unit,
    onNext: () -> Unit,
) {
    val ctx = LocalContext.current
    val scanner = remember { WifiScanner(ctx) }
    var saved by remember { mutableStateOf(m.settings.homeNetworks) }
    var nearby by remember { mutableStateOf(scanner.nearby()) }
    var query by remember { mutableStateOf("") }
    var typed by remember { mutableStateOf("") }
    // Names unticked during this visit stay listed (unticked) until the step closes, so a slip can be undone.
    var removed by remember { mutableStateOf(emptyList<String>()) }
    var note by remember { mutableStateOf<String?>(null) }
    var scanning by remember { mutableStateOf(false) }
    // The scan answers a moment later; read it then.
    LaunchedEffect(scanning) {
        if (scanning) {
            delay(3_000)
            nearby = scanner.nearby()
            scanning = false
        }
    }
    LaunchedEffect(m.locationPermitted) {
        if (m.locationPermitted) {
            nearby = scanner.nearby() // the last results are readable now
            if (scanner.rescan()) scanning = true
        }
    }

    val inRange = nearby.mapNotNull { PresenceSignals.cleanSsid(it) }.toSet()
    val choices = WifiChoices.merge(saved, m.monitor.currentNetwork(), nearby + removed, query)

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
        m.evaluatePresence()
    }

    fun addTyped() {
        val ssid = PresenceSignals.cleanSsid(typed) ?: return
        m.settings.addHome(HomeNetwork(ssid, null))
        saved = m.settings.homeNetworks
        removed = removed - ssid
        typed = ""
        query = ""
        m.evaluatePresence()
    }

    StepPage(
        title = "Home Wi-Fi · step 2 of 3",
        why = SetupText.WIFI_WHY,
        next = "Next",
        skip = if (saved.isEmpty()) "Skip, I'll do this at home" else null,
        onBack = onBack,
        onNext = onNext,
        header = {
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                OutlinedTextField(query, { query = it }, Modifier.weight(1f), label = { Text("Search networks") }, singleLine = true)
                OutlinedButton(enabled = !scanning, onClick = {
                    if (scanner.rescan()) {
                        scanning = true
                        note = null
                    } else {
                        note =
                            "Android limits how often Wi-Fi can be scanned. Showing the last results."
                    }
                }) { Text(if (scanning) "Scanning…" else "Rescan") }
            }
            if (!m.locationPermitted) FeedbackText(SetupText.WIFI_NEEDS_LOCATION, Tone.Warning)
            note?.let { Text(it, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        },
        footer = {
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                OutlinedTextField(typed, { typed = it }, Modifier.weight(1f), label = { Text("Add a network by name") }, singleLine = true)
                OutlinedButton(enabled = PresenceSignals.cleanSsid(typed) != null, onClick = { addTyped() }) { Text("Add") }
            }
        },
    ) {
        if (choices.isEmpty()) {
            Text(
                if (query.isBlank()) SetupText.WIFI_NONE_FOUND else "Nothing matches \"$query\".",
                fontSize = 12.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        choices.forEach { c ->
            Row(Modifier.fillMaxWidth().clickable { toggle(c, !c.saved) }, verticalAlignment = Alignment.CenterVertically) {
                Checkbox(c.saved, { toggle(c, it) })
                Column {
                    Text(c.ssid)
                    val tag =
                        when {
                            c.connected -> "Connected now"
                            c.saved && nearby.isNotEmpty() && c.ssid !in inRange -> "Saved, not in range"
                            else -> null
                        }
                    tag?.let { Text(it, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                }
            }
        }
    }
}

/** Step 3: tick the Bluetooth device that is your car. Optional. */
@Composable
internal fun CarStep(
    m: AppModel,
    onBack: () -> Unit,
    onDone: () -> Unit,
) {
    val ctx = LocalContext.current
    var car by remember { mutableStateOf(m.settings.carDevices) }
    var query by remember { mutableStateOf("") }
    var btOk by remember {
        mutableStateOf(
            Build.VERSION.SDK_INT < 31 || ctx.checkSelfPermission(
                Manifest.permission.BLUETOOTH_CONNECT,
            ) == PackageManager.PERMISSION_GRANTED,
        )
    }
    // Tell the model too: it restarts the monitor so car detection works without leaving the app.
    val askBt =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
            btOk = it
            m.ensureMonitor(it)
        }
    val paired =
        remember(btOk) {
            if (!btOk) {
                emptyList()
            } else {
                runCatching {
                    ctx
                        .getSystemService(
                            BluetoothManager::class.java,
                        )?.adapter
                        ?.bondedDevices
                        ?.map { CarDevice(it.name ?: it.address, it.address) }
                        ?: emptyList()
                }.getOrDefault(emptyList())
            }
        }

    fun toggle(
        d: CarDevice,
        on: Boolean,
    ) {
        val now = m.settings.carDevices.filterNot { it.address == d.address }
        m.settings.setCar(if (on) now + d else now)
        car = m.settings.carDevices
        m.evaluatePresence()
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
            OutlinedButton(onClick = { askBt.launch(Manifest.permission.BLUETOOTH_CONNECT) }) { Text("Allow Bluetooth to pick your car") }
            Text(SetupText.CAR_NEEDS_BLUETOOTH, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        } else if (paired.isEmpty() &&
            car.isEmpty()
        ) {
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
