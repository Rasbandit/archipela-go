package dev.apgo2

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import android.os.BatteryManager
import android.os.Build
import android.os.PowerManager

private const val PROGRESS_BUCKETS = 10
private const val REJECT_ACCURACY_M = 35f
private const val MS_PER_SECOND = 1000

/** What the phone delivered since the last heartbeat line, and the other facts that explain a gap in the diagnostics log. */
internal class FieldDiagnostics(
    private val model: AppModel,
    private val ctx: Context,
) {
    private var fixesSinceBeat = 0
    private var rejectedSinceBeat = 0
    private var lastFixMs = 0L
    private var lastFixAcc = 0f
    private var lastFixProvider = ""
    private val progressBuckets = HashMap<Long, Int>()
    private val providerCounts = HashMap<String, Int>()

    /** Count a location fix for the next heartbeat. */
    fun recordFix(loc: Location) {
        lastFixMs = model.now()
        lastFixAcc = loc.accuracy
        lastFixProvider = loc.provider ?: ""
        if (loc.accuracy > REJECT_ACCURACY_M) rejectedSinceBeat++ else fixesSinceBeat++
        providerCounts.merge(loc.provider ?: "?", 1, Int::plus)
    }

    /** Log quest progress each time it crosses a 10% step, so a quest that never moves shows up in the log. */
    fun logProgress() {
        for (q in model.quests) {
            if (q.state != "progress") {
                progressBuckets.remove(q.locationId)
                continue
            }
            val bucket = (q.progress * PROGRESS_BUCKETS).toInt()
            if (progressBuckets.put(q.locationId, bucket) != bucket) {
                Diag.info("progress", q.name, "kind" to q.kindId, "percent" to bucket * PROGRESS_BUCKETS, "id" to q.locationId)
            }
        }
    }

    /** One line a minute while a game is open: what the sensors delivered, power state and battery. Gaps in these lines are the story. */
    fun heartbeat() {
        Diag.info("heartbeat", "tracking", *sensorFields(), *deviceFields(), *gameFields())
        fixesSinceBeat = 0
        rejectedSinceBeat = 0
        providerCounts.clear()
        model.presence.evaluate() // backstop: a settled debounce never waits longer than a minute
        drainCore()
    }

    /** Move messages the Rust core queued (journal failures etc.) into the diagnostics log. */
    fun drainCore() = model.engine.takeDiag().forEach { Diag.warn("core", it) }

    private fun sensorFields(): Array<Pair<String, Any?>> =
        arrayOf(
            "fixes" to fixesSinceBeat,
            "rejected" to rejectedSinceBeat,
            "last_fix_age_s" to if (lastFixMs == 0L) -1L else (model.now() - lastFixMs) / MS_PER_SECOND,
            "last_acc_m" to lastFixAcc,
            "last_provider" to lastFixProvider,
            "by_provider" to providerCounts.entries.joinToString(",") { "${it.key}=${it.value}" },
            "steps" to model.stepsTotal,
        )

    private fun deviceFields(): Array<Pair<String, Any?>> {
        val pm = ctx.getSystemService(Context.POWER_SERVICE) as PowerManager
        val bm = ctx.getSystemService(Context.BATTERY_SERVICE) as BatteryManager
        val backgroundLocation =
            Build.VERSION.SDK_INT < Build.VERSION_CODES.Q ||
                ctx.checkSelfPermission(Manifest.permission.ACCESS_BACKGROUND_LOCATION) == PackageManager.PERMISSION_GRANTED
        return arrayOf(
            "battery_pct" to bm.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY),
            "screen_on" to pm.isInteractive,
            "power_save" to pm.isPowerSaveMode,
            "doze" to pm.isDeviceIdleMode,
            "bg_location" to backgroundLocation,
            "unrestricted" to pm.isIgnoringBatteryOptimizations(ctx.packageName),
        )
    }

    private fun gameFields(): Array<Pair<String, Any?>> =
        arrayOf(
            "quests" to model.quests.size,
            "done" to (model.hud?.done ?: 0),
            "presence" to model.presence.decision.state.name,
            "counting" to model.presence.decision.counting,
        )
}
