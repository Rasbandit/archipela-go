package dev.apgo2

import android.Manifest
import android.bluetooth.BluetoothManager
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Checkbox
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.presence.CarDevice
import dev.apgo2.presence.HomeNetwork
import dev.apgo2.presence.PresenceSignals
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.FeedbackText
import dev.apgo2.ui.Tone

/** Where the player says which Wi-Fi is home and which Bluetooth device is the car. */
@Composable
fun PresenceScreen(m: AppModel) {
    val ctx = LocalContext.current
    var home by remember { mutableStateOf(m.settings.homeNetworks) }
    var car by remember { mutableStateOf(m.settings.carDevices) }
    var problem by remember { mutableStateOf<String?>(null) }
    var btOk by remember { mutableStateOf(Build.VERSION.SDK_INT < 31 || ctx.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED) }
    // Tell the model too: it restarts the monitor so car detection works without leaving the app.
    val askBt = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { btOk = it; m.ensureMonitor(it) }
    val paired = remember(btOk) {
        if (!btOk) emptyList()
        else runCatching { ctx.getSystemService(BluetoothManager::class.java)?.adapter?.bondedDevices?.map { CarDevice(it.name ?: it.address, it.address) } ?: emptyList() }.getOrDefault(emptyList())
    }
    Column(Modifier.fillMaxSize().statusBarsPadding().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Text("Presence", style = MaterialTheme.typography.titleLarge)
            TextButton(onClick = { m.showPresence = false }) { Text("Done") }
        }
        Text("Home Wi-Fi networks", style = MaterialTheme.typography.titleMedium)
        Text("While connected to one of these, nothing counts and GPS is off.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        home.forEach { n ->
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(n.ssid, Modifier.weight(1f))
                IconButton(onClick = { m.settings.removeHome(n.ssid); home = m.settings.homeNetworks; m.evaluatePresence() }) { Icon(ApgoIcons.Close, contentDescription = "Remove ${n.ssid}") }
            }
        }
        OutlinedButton(onClick = {
            val w = m.monitor.currentNetwork()
            val ssid = PresenceSignals.cleanSsid(w?.ssid)
            if (ssid == null) problem = "Connect to your home Wi-Fi first (and allow location)"
            else { problem = null; m.settings.addHome(HomeNetwork(ssid, w?.bssid)); home = m.settings.homeNetworks; m.evaluatePresence() }
        }) { Text("Add current network") }
        problem?.let { FeedbackText(it, Tone.Warning) }
        HorizontalDivider()
        Text("Car Bluetooth", style = MaterialTheme.typography.titleMedium)
        Text("While one of these devices is connected, nothing counts (no pickups while driving).", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        if (!btOk) OutlinedButton(onClick = { askBt.launch(Manifest.permission.BLUETOOTH_CONNECT) }) { Text("Allow Bluetooth to pick your car") }
        else if (paired.isEmpty()) Text("No paired Bluetooth devices found. Pair your car in the phone's Bluetooth settings first.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        paired.forEach { d ->
            val on = car.any { it.address == d.address }
            Row(Modifier.fillMaxWidth().clickable { toggleCar(m, d, !on) { car = it } }, verticalAlignment = Alignment.CenterVertically) {
                Checkbox(on, { toggleCar(m, d, it) { updated -> car = updated } })
                Text(d.name)
            }
        }
    }
}

private fun toggleCar(m: AppModel, d: CarDevice, on: Boolean, done: (List<CarDevice>) -> Unit) {
    val now = m.settings.carDevices
    m.settings.setCar(if (on) now.filterNot { it.address == d.address } + d else now.filterNot { it.address == d.address })
    done(m.settings.carDevices)
    m.evaluatePresence()
}
