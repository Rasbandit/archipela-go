package dev.apgo2

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.location.LocationListener
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.location.LocationManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.fillMaxSize
import dev.apgo2.ui.ApgoTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.statusBarsPadding
import kotlinx.coroutines.delay

class MainActivity : ComponentActivity() {
    private fun providers(lm: LocationManager): List<String> {
        val all = buildList {
            if (Build.VERSION.SDK_INT >= 31) add(LocationManager.FUSED_PROVIDER)
            add(LocationManager.GPS_PROVIDER)
            add(LocationManager.NETWORK_PROVIDER)
        }
        return all.filter { lm.isProviderEnabled(it) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            ApgoTheme {
                Surface(Modifier.fillMaxSize().statusBarsPadding()) {
                    val scope = rememberCoroutineScope()
                    val model = remember { AppModel(applicationContext, scope).also { it.refreshAll() } }
                    var permitted by remember { mutableStateOf(false) }
                    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { permitted = it }
                    LaunchedEffect(Unit) { ask.launch(Manifest.permission.ACCESS_FINE_LOCATION) }
                    var stepsOk by remember { mutableStateOf(false) }
                    val askSteps = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { stepsOk = it }
                    LaunchedEffect(permitted) { if (permitted) askSteps.launch(Manifest.permission.ACTIVITY_RECOGNITION) }
                    DisposableEffect(stepsOk) {
                        if (!stepsOk) return@DisposableEffect onDispose {}
                        val sm = getSystemService(Context.SENSOR_SERVICE) as SensorManager
                        val sensor = sm.getDefaultSensor(Sensor.TYPE_STEP_COUNTER)
                        val l = object : SensorEventListener {
                            override fun onSensorChanged(e: SensorEvent) { model.stepsTotal = e.values[0].toLong() }
                            override fun onAccuracyChanged(s: Sensor?, a: Int) {}
                        }
                        if (sensor != null) sm.registerListener(l, sensor, SensorManager.SENSOR_DELAY_NORMAL)
                        onDispose { sm.unregisterListener(l) }
                    }

                    DisposableEffect(permitted) {
                        if (!permitted) return@DisposableEffect onDispose {}
                        val lm = getSystemService(Context.LOCATION_SERVICE) as LocationManager
                        val listener = LocationListener { loc -> model.realLoc = loc; model.onFix(loc) }
                        // Listen on every enabled provider: whichever has a fix wins (emulators only feed GPS).
                        providers(lm).forEach { provider ->
                            @SuppressLint("MissingPermission")
                            lm.requestLocationUpdates(provider, 1000L, 0f, listener)
                            @SuppressLint("MissingPermission")
                            lm.getLastKnownLocation(provider)?.let { model.realLoc = it }
                        }
                        onDispose { lm.removeUpdates(listener) }
                    }
                    LaunchedEffect(model.session) { while (model.session != null) { model.apTick(); delay(300) } }
                    AppRoot(model)
                }
            }
        }
    }
}
