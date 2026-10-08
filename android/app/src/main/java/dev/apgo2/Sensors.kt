package dev.apgo2

import android.annotation.SuppressLint
import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.location.LocationListener
import android.location.LocationManager
import android.os.Build

/**
 * Location and step-counter listeners. They belong to the app model, not to the activity, so they keep running
 * (with [TrackingService] holding the process in the foreground) when the screen is off or the activity is gone.
 * Callers must hold the matching permissions before calling the start functions.
 */
class Sensors(private val ctx: Context, private val model: AppModel) {
    private val lm = ctx.getSystemService(Context.LOCATION_SERVICE) as LocationManager
    private val sm = ctx.getSystemService(Context.SENSOR_SERVICE) as SensorManager
    private var locationListener: LocationListener? = null
    private var rate: GpsPolicy.Rate? = null
    private var stepListener: SensorEventListener? = null

    private fun providers(): List<String> {
        val enabled = lm.allProviders.filter { lm.isProviderEnabled(it) }.toSet()
        return GpsPolicy.providers(enabled, Build.VERSION.SDK_INT)
    }

    /** Start (or re-start with a new [rate]). Calling again with the same rate does nothing. */
    @SuppressLint("MissingPermission")
    fun startLocation(rate: GpsPolicy.Rate) {
        if (locationListener != null && this.rate == rate) return
        stopLocation()
        val l = LocationListener { loc -> model.realLoc = loc; model.onFix(loc) }
        // One provider only: mixing them interleaved 100 m-off network fixes with good GPS fixes and made the position jump streets.
        var registered = false
        providers().forEach { p ->
            runCatching {
                lm.requestLocationUpdates(p, rate.intervalMs, rate.minDistanceM, l)
                registered = true
                lm.getLastKnownLocation(p)?.let { model.realLoc = it }
            }.onFailure { Diag.e("sensors", "requestLocationUpdates failed for $p", it) }
        }
        // Nothing registered (no permission yet): remember nothing, so a later call with the same rate tries again.
        if (!registered) return lm.removeUpdates(l)
        locationListener = l
        this.rate = rate
        Diag.i("sensors", "location started", "interval_ms" to rate.intervalMs, "min_dist_m" to rate.minDistanceM, "providers" to providers().joinToString(","))
    }

    fun stopLocation() {
        if (locationListener != null) Diag.i("sensors", "location stopped")
        locationListener?.let { lm.removeUpdates(it) }
        locationListener = null
        rate = null
    }

    fun startSteps() {
        if (stepListener != null) return
        val sensor = sm.getDefaultSensor(Sensor.TYPE_STEP_COUNTER) ?: return Diag.w("sensors", "no step counter on this device")
        val l = object : SensorEventListener {
            override fun onSensorChanged(e: SensorEvent) { model.onSteps(e.values[0].toLong()) }
            override fun onAccuracyChanged(s: Sensor?, a: Int) {}
        }
        sm.registerListener(l, sensor, SensorManager.SENSOR_DELAY_NORMAL)
        stepListener = l
        Diag.i("sensors", "steps started")
    }

    fun stopSteps() {
        stepListener?.let { sm.unregisterListener(it) }
        stepListener = null
    }
}
