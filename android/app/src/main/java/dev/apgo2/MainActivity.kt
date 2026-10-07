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
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.apgo_ffi.FillMode
import uniffi.apgo_ffi.GeoPoint
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
        var status by remember { mutableStateOf("Tap 'Locate me' first.") }
        var trips by remember { mutableStateOf<List<TripOut>>(emptyList()) }

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

        Column(Modifier.statusBarsPadding().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Archipela-Go 2 spike", style = MaterialTheme.typography.headlineSmall)
            Text(coreVersion())
            Text(status)
            Button(onClick = { askLocation.launch(Manifest.permission.ACCESS_FINE_LOCATION) }) { Text("Locate me") }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { generate(FillMode.CELLS) }) { Text("100 trips: cells") }
                Button(onClick = { generate(FillMode.STREETS) }) { Text("100 trips: streets") }
            }
            LazyColumn {
                items(trips.take(30)) { t ->
                    Text("#${t.number} tier ${t.tier}  ${"%.0f".format(t.distanceM)} m  ${t.name}")
                }
            }
        }
    }
}
