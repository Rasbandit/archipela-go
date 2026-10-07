package dev.apgo2

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.location.Location
import android.location.LocationManager
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import uniffi.apgo_ffi.ApEvent
import uniffi.apgo_ffi.ApSession
import uniffi.apgo_ffi.FillMode
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.ReceivedItemOut
import uniffi.apgo_ffi.TripOut
import uniffi.apgo_ffi.coreVersion
import uniffi.apgo_ffi.generateTrips

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { MaterialTheme { Surface(Modifier.fillMaxSize()) { Spike() } } }
    }

    @SuppressLint("MissingPermission")
    fun locate(onResult: (Location?) -> Unit) {
        val lm = getSystemService(Context.LOCATION_SERVICE) as LocationManager
        val providers = buildList {
            if (Build.VERSION.SDK_INT >= 31) add(LocationManager.FUSED_PROVIDER)
            add(LocationManager.GPS_PROVIDER)
            add(LocationManager.NETWORK_PROVIDER)
        }.filter { lm.isProviderEnabled(it) }
        val provider = providers.firstOrNull() ?: return onResult(null)
        lm.getCurrentLocation(provider, null, mainExecutor) { loc -> onResult(loc ?: lm.getLastKnownLocation(provider)) }
    }

    @Composable
    fun Spike() {
        val scope = rememberCoroutineScope()
        var here by remember { mutableStateOf<Location?>(null) }
        var status by remember { mutableStateOf("Tap 'Locate me', then generate trips.") }
        var trips by remember { mutableStateOf<List<TripOut>>(emptyList()) }

        var url by remember { mutableStateOf("localhost:38281") }
        var slot by remember { mutableStateOf("Tester") }
        var session by remember { mutableStateOf<ApSession?>(null) }
        var apStatus by remember { mutableStateOf("not connected") }
        var slotInfo by remember { mutableStateOf("") }
        var slotTripIds by remember { mutableStateOf<List<Long>>(emptyList()) }
        var nextCheck by remember { mutableStateOf(0) }
        var items by remember { mutableStateOf<List<ReceivedItemOut>>(emptyList()) }
        val log = remember { mutableStateListOf<String>() }

        // Poll the Archipelago connection a few times a second.
        LaunchedEffect(session) {
            val s = session ?: return@LaunchedEffect
            while (true) {
                val events = withContext(Dispatchers.IO) {
                    runCatching { s.poll() }.getOrElse { listOf(ApEvent.Error("poll failed: ${it.message}")) }
                }
                events.forEach { e ->
                    when (e) {
                        is ApEvent.Print -> log.add(e.text)
                        is ApEvent.Error -> log.add("ERROR: ${e.detail}")
                        is ApEvent.Connected -> {
                            val data = s.slotDataJson()?.let { JSONObject(it) }
                            val arr = data?.optJSONArray("trips")
                            slotTripIds = (0 until (arr?.length() ?: 0)).map { arr!!.getJSONObject(it).getLong("location_id") }
                            slotInfo = data?.let { "goal=${it.optString("goal")} trips=${slotTripIds.size} schema=${it.optInt("schema_version")}" } ?: "no slot_data"
                        }
                        else -> {}
                    }
                }
                apStatus = s.status()
                items = withContext(Dispatchers.IO) { s.receivedItems() }
                delay(250)
            }
        }

        val askLocation = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            if (granted) locate { here = it; status = it?.let { l -> "Located: %.5f, %.5f (±%.0f m)".format(l.latitude, l.longitude, l.accuracy) } ?: "No location fix yet" }
            else status = "Location permission denied"
        }

        fun generate(mode: FillMode) {
            val loc = here ?: return run { status = "Locate first" }
            status = "Generating (${mode.name.lowercase()})..."
            scope.launch {
                val t0 = SystemClock.elapsedRealtime()
                val result = withContext(Dispatchers.IO) {
                    runCatching {
                        generateTrips(GeoPoint(loc.latitude, loc.longitude), 5000.0, 100u, 1uL, mode, cacheDir.absolutePath)
                    }
                }
                val ms = SystemClock.elapsedRealtime() - t0
                result.onSuccess {
                    trips = it
                    status = "${it.size} trips (${it.count { t -> t.inBand }} in band) in $ms ms"
                }.onFailure { status = "Failed: ${it.message}" }
            }
        }

        Column(Modifier.statusBarsPadding().padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text("Archipela-Go 2 spike", style = MaterialTheme.typography.headlineSmall)
            Text(coreVersion())
            Text(status)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { askLocation.launch(Manifest.permission.ACCESS_FINE_LOCATION) }) { Text("Locate me") }
                Button(onClick = { generate(FillMode.CELLS) }) { Text("Trips: cells") }
                Button(onClick = { generate(FillMode.STREETS) }) { Text("Trips: streets") }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(url, { url = it }, label = { Text("Server") }, singleLine = true, modifier = Modifier.weight(1f))
                OutlinedTextField(slot, { slot = it }, label = { Text("Slot") }, singleLine = true, modifier = Modifier.weight(1f))
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = {
                    log.clear(); items = emptyList(); nextCheck = 0
                    session = ApSession.connect(url, slot, null, cacheDir.resolve("ap").absolutePath)
                }) { Text("Connect") }
                Button(onClick = {
                    val s = session ?: return@Button
                    val id = slotTripIds.getOrNull(nextCheck) ?: return@Button
                    runCatching { s.sendCheck(id) }
                        .onSuccess { log.add("sent check #${nextCheck + 1} (location $id)"); nextCheck++ }
                        .onFailure { log.add("check failed: ${it.message}") }
                }) { Text("Check next trip") }
            }
            Text("AP: $apStatus  $slotInfo")
            LazyColumn {
                items(items.takeLast(6)) { Text("item: ${it.name} (from ${it.sender})${if (it.progression) " [prog]" else ""}") }
                items(log.takeLast(8)) { Text(it) }
                items(trips.take(5)) { t -> Text("#${t.number} tier ${t.tier}  ${"%.0f".format(t.distanceM)} m  ${t.name}") }
            }
        }
    }
}
