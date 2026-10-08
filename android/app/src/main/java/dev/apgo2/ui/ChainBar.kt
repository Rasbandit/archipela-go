package dev.apgo2.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp

/** One bar with a mark at each check it unlocks: filled up to [fill], reached marks solid green, the others hollow. */
@Composable
internal fun ChainBar(
    fill: Float,
    fractions: List<Float>,
    reached: List<Boolean>,
    modifier: Modifier = Modifier,
) {
    val track = ApgoPalette.mint
    val filled = ApgoPalette.teal
    val done = ApgoPalette.questDone
    Canvas(modifier.fillMaxWidth().height(18.dp)) {
        val h = 6.dp.toPx()
        val top = (size.height - h) / 2
        drawRoundRect(track, Offset(0f, top), Size(size.width, h), CornerRadius(h / 2))
        drawRoundRect(filled, Offset(0f, top), Size(size.width * fill, h), CornerRadius(h / 2))
        val r = 6.dp.toPx()
        fractions.forEachIndexed { i, f ->
            val x = (size.width * f).coerceIn(r, size.width - r)
            val c = Offset(x, size.height / 2)
            drawCircle(ApgoPalette.onMap, r, c)
            if (reached.getOrElse(i) {
                    false
                }
            ) {
                drawCircle(
                    done,
                    r - 1.5.dp.toPx(),
                    c,
                )
            } else {
                drawCircle(ApgoPalette.muted, r - 1.5.dp.toPx(), c, style = Stroke(1.5.dp.toPx()))
            }
        }
    }
}
