package dev.apgo2

import android.content.Context
import androidx.core.content.edit
import uniffi.apgo_ffi.Engine
import uniffi.apgo_ffi.StepCalIn
import uniffi.apgo_ffi.StepCalOut

/** The phone's step source; a watch or Health Connect source would get its own key and calibration. */
internal const val PHONE_STEP_COUNTER = "phone.step_counter"

private const val FIELDS = 4

/** `k;var_k;samples;updated_ms` (pure, unit-tested); a scale or variance that is not a finite number (variance > 0) is no value. */
internal object StepCalCodec {
    fun encode(c: StepCalOut): String = listOf(c.k, c.varK, c.samples, c.updatedMs).joinToString(";")

    fun decode(
        source: String,
        text: String?,
    ): StepCalIn? {
        val p = text?.split(";")?.takeIf { it.size == FIELDS } ?: return null
        return runCatching { StepCalIn(source, p[0].toDouble(), p[1].toDouble(), p[2].toUInt(), p.last().toLong()) }
            .getOrNull()
            ?.takeIf { it.k.isFinite() && it.varK.isFinite() && it.varK > 0.0 }
    }
}

/** Step calibrations in app preferences (`stepcal`), one entry per step source; never in the game save. */
internal class StepCalStore(
    ctx: Context,
) {
    private val prefs = ctx.getSharedPreferences("stepcal", Context.MODE_PRIVATE)

    fun load(source: String): StepCalIn? = StepCalCodec.decode(source, prefs.getString(source, null))

    fun save(c: StepCalOut) {
        prefs.edit { putString(c.source, StepCalCodec.encode(c)) }
    }

    /** Load the phone's saved calibration into the game just opened or started. */
    fun loadInto(engine: Engine) {
        load(PHONE_STEP_COUNTER)?.let(engine::setStepCalibration)
    }

    /** Save the open game's calibration (on pause, background and every 5 minutes while playing); nothing before it learned. */
    fun saveFrom(engine: Engine) {
        engine.stepCalibration()?.let(::save)
    }
}
