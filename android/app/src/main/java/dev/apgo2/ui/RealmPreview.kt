package dev.apgo2.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.background
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import kotlin.math.cos
import kotlin.math.max

/** One find on a preview: where it is and the colour of its kind. */
data class PreviewDot(val lat: Double, val lon: Double, val color: Color)

/**
 * A small schematic of a realm: its outline with its finds as dots in their kind colours. Drawn, not fetched, so it is instant, works offline
 * and follows the theme.
 */
@Composable
fun RealmPreview(outline: List<Pair<Double, Double>>, dots: List<PreviewDot>, modifier: Modifier = Modifier) {
    val ink = MaterialTheme.colorScheme.primary
    Canvas(modifier.clip(RoundedCornerShape(12.dp)).background(MaterialTheme.colorScheme.surfaceVariant)) {
        if (outline.size < 3) return@Canvas
        // Equirectangular: longitude shrinks with latitude. Fit the outline in the square with a margin, keeping its aspect.
        val midLat = outline.map { it.first }.average()
        val k = cos(Math.toRadians(midLat))
        val xs = outline.map { it.second * k }
        val ys = outline.map { -it.first }
        val (minX, maxX, minY, maxY) = listOf(xs.min(), xs.max(), ys.min(), ys.max())
        val margin = size.minDimension * 0.12f
        val scale = (size.minDimension - 2 * margin) / max(max(maxX - minX, maxY - minY), 1e-9).toFloat()
        val offX = (size.width - (maxX - minX).toFloat() * scale) / 2f
        val offY = (size.height - (maxY - minY).toFloat() * scale) / 2f
        fun at(lat: Double, lon: Double) = Offset(offX + ((lon * k) - minX).toFloat() * scale, offY + ((-lat) - minY).toFloat() * scale)

        val shape = Path().apply {
            outline.forEachIndexed { i, (lat, lon) -> at(lat, lon).let { if (i == 0) moveTo(it.x, it.y) else lineTo(it.x, it.y) } }
            close()
        }
        drawPath(shape, ink.copy(alpha = 0.12f))
        drawPath(shape, ink, style = Stroke(width = 2.dp.toPx()))
        dots.forEach { drawCircle(it.color, radius = 1.8.dp.toPx(), center = at(it.lat, it.lon)) }
    }
}
