package dev.apgo2.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

private const val MARK_MIN = 0.03f
private const val MARK_MAX = 0.97f
private const val BIT_COUNT = 14
private const val BOUNCE_PEAK = 1.7f
private const val BOUNCE_UP_MS = 160
private const val CONFETTI_MS = 820
private const val CONFETTI_FADE_FROM = 0.7f
private const val GAIN_OVERLAP = 0.02f
private const val BOUNCE_DAMPING = 0.5f
private const val BURST_VISIBLE_MIN = 0.001f
private const val BURST_VISIBLE_MAX = 0.999f

/** One confetti piece: colour, end offset from the dot (dp) and end rotation (degrees). */
internal data class Bit(
    val color: Color,
    val dx: Float,
    val dy: Float,
    val rotation: Float,
)

/** Where a mark sits on the bar, clamped so its dot never leaves the track. */
internal fun markFraction(
    mark: Float,
    max: Float,
): Float = if (max <= 0f) MARK_MIN else (mark / max).coerceIn(MARK_MIN, MARK_MAX)

/** Indexes of marks passed on this walk: above [from], at or below [value]. */
internal fun crossed(
    marks: List<Float>,
    from: Float,
    value: Float,
): List<Int> = marks.indices.filter { marks[it] > from && marks[it] <= value }

private val BIT_COLORS =
    listOf(
        ApgoPalette.questProgress,
        ApgoPalette.questDone,
        ApgoPalette.sky,
        ApgoPalette.butter,
        ApgoPalette.questTodo,
        ApgoPalette.grass,
    )

internal val CONFETTI: List<Bit> =
    List(BIT_COUNT) { i ->
        val angle = i.toDouble() / BIT_COUNT * 2 * PI + if (i % 2 == 1) 0.25 else -0.1
        val radius = 16 + i * 7 % 5 * 5
        Bit(
            color = BIT_COLORS[i % BIT_COLORS.size],
            dx = (cos(angle) * radius).toFloat(),
            dy = (sin(angle) * radius * 0.8 + 6).toFloat(),
            rotation = (i * 97 % 360 - 180).toFloat(),
        )
    }

private class DotAnims(
    n: Int,
) {
    val scales = List(n) { Animatable(1f) }
    val bursts = List(n) { Animatable(0f) }
    val fired = mutableSetOf<Int>()

    /** Static end state: dots at rest, confetti finished (not drawn). */
    suspend fun settle() {
        scales.forEach { it.snapTo(1f) }
        bursts.forEach { it.snapTo(1f) }
    }
}

/** Which crossed indexes begin their bounce now: none when not animating, each index only once. */
internal fun startingNow(
    crossed: List<Int>,
    fired: Set<Int>,
    animate: Boolean,
): List<Int> = if (animate) crossed.filter { it !in fired } else emptyList()

private fun unitFraction(
    units: Float,
    max: Float,
): Float = if (max <= 0f) 0f else (units / max).coerceIn(0f, 1f)

/**
 * Chain bar for the walk recap: a blue "before" fill, an orange "this walk" fill up to [value], and a bounce plus confetti
 * on every mark in `(crossedFrom, value]` the first time [value] reaches it (only when [animate]).
 */
@Composable
internal fun RecapChainBar(
    before: Float,
    value: Float,
    max: Float,
    marks: List<Float>,
    crossedFrom: Float,
    animate: Boolean,
    modifier: Modifier = Modifier,
) {
    val anims = remember(marks.size) { DotAnims(marks.size) }
    val scope = rememberCoroutineScope()
    val newly = crossed(marks, crossedFrom, value)
    LaunchedEffect(newly, animate) {
        if (!animate) {
            anims.settle()
        } else {
            startingNow(newly, anims.fired, true).forEach {
                anims.fired.add(it)
                scope.launch { bounce(anims, it) }
            }
        }
    }
    Canvas(modifier.fillMaxWidth().height(18.dp)) {
        val h = 6.dp.toPx()
        val top = (size.height - h) / 2
        val corner = CornerRadius(h / 2)
        drawRoundRect(ApgoPalette.mint, Offset(0f, top), Size(size.width, h), corner)
        val gainFrom = (unitFraction(before, max) - GAIN_OVERLAP).coerceAtLeast(0f)
        drawFill(gainFrom, unitFraction(value, max), ApgoPalette.questProgress, top, h)
        drawFill(0f, unitFraction(before, max), ApgoPalette.questTodo, top, h)
        val r = 6.dp.toPx()
        marks.forEachIndexed { i, mark -> drawDot(dotCentre(mark, max, r), r, anims.scales[i].value, mark <= value) }
        anims.bursts.forEachIndexed { i, burst ->
            if (burst.value in BURST_VISIBLE_MIN..BURST_VISIBLE_MAX) drawBurst(dotCentre(marks[i], max, r), burst.value)
        }
    }
}

private suspend fun bounce(
    anims: DotAnims,
    i: Int,
) = coroutineScope {
    launch {
        anims.scales[i].animateTo(BOUNCE_PEAK, tween(BOUNCE_UP_MS, easing = FastOutSlowInEasing))
        anims.scales[i].animateTo(1f, spring(dampingRatio = BOUNCE_DAMPING, stiffness = Spring.StiffnessMedium))
    }
    launch { anims.bursts[i].animateTo(1f, tween(CONFETTI_MS, easing = FastOutSlowInEasing)) }
}

private fun DrawScope.dotCentre(
    mark: Float,
    max: Float,
    r: Float,
) = Offset((size.width * markFraction(mark, max)).coerceIn(r, size.width - r), size.height / 2)

private fun DrawScope.drawFill(
    from: Float,
    to: Float,
    color: Color,
    top: Float,
    h: Float,
) {
    val x0 = size.width * from
    val x1 = size.width * to
    if (x1 > x0) drawRoundRect(color, Offset(x0, top), Size(x1 - x0, h), CornerRadius(h / 2))
}

private fun DrawScope.drawDot(
    c: Offset,
    r: Float,
    scale: Float,
    reached: Boolean,
) {
    drawCircle(ApgoPalette.onMap, r * scale, c)
    val inner = r - 1.5.dp.toPx()
    if (reached) {
        drawCircle(ApgoPalette.questDone, inner * scale, c)
    } else {
        drawCircle(ApgoPalette.muted, inner, c, style = Stroke(1.5.dp.toPx()))
    }
}

private fun DrawScope.drawBurst(
    c: Offset,
    p: Float,
) {
    val alpha = if (p < CONFETTI_FADE_FROM) 1f else 1f - (p - CONFETTI_FADE_FROM) / (1f - CONFETTI_FADE_FROM)
    val w = 6.dp.toPx()
    val hh = 4.dp.toPx()
    CONFETTI.forEach { bit ->
        val centre = Offset(c.x + bit.dx.dp.toPx() * p, c.y + bit.dy.dp.toPx() * p)
        rotate(bit.rotation * p, centre) {
            drawRoundRect(bit.color, Offset(centre.x - w / 2, centre.y - hh / 2), Size(w, hh), CornerRadius(1.dp.toPx()), alpha = alpha)
        }
    }
}
