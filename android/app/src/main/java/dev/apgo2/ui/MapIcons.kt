package dev.apgo2.ui

import android.graphics.Bitmap
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.VectorGroup
import androidx.compose.ui.graphics.vector.VectorPath
import androidx.compose.ui.graphics.vector.toPath
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection

/*
 * MapLibre symbols are bitmaps, so the Lucide icons the UI uses are drawn into bitmaps here. Lucide icons are stroked paths on a 24x24 grid.
 */

private fun DrawScope.strokeIcon(group: VectorGroup, color: Color, width: Float = 2f) {
    group.forEach { node ->
        when (node) {
            is VectorPath -> drawPath(node.pathData.toPath(), color, style = Stroke(width = width, cap = StrokeCap.Round, join = StrokeJoin.Round))
            is VectorGroup -> strokeIcon(node, color, width)
        }
    }
}

private fun render(sizePx: Int, draw: DrawScope.() -> Unit): Bitmap {
    val image = ImageBitmap(sizePx, sizePx)
    CanvasDrawScope().draw(Density(1f), LayoutDirection.Ltr, Canvas(image), Size(sizePx.toFloat(), sizePx.toFloat()), draw)
    return image.asAndroidBitmap()
}

private fun DrawScope.icon(icon: ImageVector, at: Float, sizePx: Float, color: Color, width: Float = 2f) {
    withTransform({
        translate(left = at, top = at)
        scale(sizePx / icon.viewportWidth, sizePx / icon.viewportHeight, pivot = Offset.Zero)
    }) { strokeIcon(icon.root, color, width) }
}

/** A round badge with the icon inside: the pin for a find on the map. */
fun renderPin(icon: ImageVector, sizePx: Int, fill: Color, glyph: Color = Color.White, ring: Color = Color.White, ringFraction: Float = 0.07f): Bitmap = render(sizePx) {
    val s = sizePx.toFloat()
    drawCircle(ring, radius = s / 2)
    drawCircle(fill, radius = s / 2 - s * ringFraction)
    val inner = s * 0.54f
    icon(icon, (s - inner) / 2, inner, glyph)
}

/** Just the icon, for drawing a glyph on top of another marker. */
fun renderGlyph(icon: ImageVector, sizePx: Int, color: Color = Color.White): Bitmap = render(sizePx) { icon(icon, 0f, sizePx.toFloat(), color) }

/** The icon itself, bold and with a light outline so it reads on any map: a marker that is the shape (a house) rather than a badge around it. */
fun renderMarker(icon: ImageVector, sizePx: Int, color: Color, outline: Color = Color.White): Bitmap = render(sizePx) {
    val s = sizePx.toFloat()
    val inset = s * 0.1f
    icon(icon, inset, s - 2 * inset, outline, width = 4.6f)
    icon(icon, inset, s - 2 * inset, color, width = 2.4f)
}

/** A find pin with the quest's state as a badge in the bottom-right corner: a check when done, an amber dot in progress, a lock when locked. */
fun renderQuestPin(icon: ImageVector, sizePx: Int, fill: Color, badge: MapMarkers.Badge): Bitmap = render(sizePx) {
    val s = sizePx.toFloat()
    val body = s * 0.86f // leave room for the badge to sit on the corner
    drawCircle(Color.White, radius = body / 2, center = Offset(body / 2, body / 2))
    drawCircle(fill, radius = body / 2 - body * 0.07f, center = Offset(body / 2, body / 2))
    val inner = body * 0.54f
    icon(icon, (body - inner) / 2, inner, Color.White)
    if (badge != MapMarkers.Badge.None) {
        val r = s * 0.2f
        val c = Offset(s - r, s - r)
        drawCircle(Color.White, radius = r, center = c)
        val color = when (badge) {
            MapMarkers.Badge.Done -> ApgoPalette.questDone
            MapMarkers.Badge.Progress -> ApgoPalette.questProgress
            else -> ApgoPalette.questLocked
        }
        drawCircle(color, radius = r - s * 0.025f, center = c)
        val g = r * 1.15f
        when (badge) {
            MapMarkers.Badge.Done -> icon(ApgoIcons.Check, c.x - g / 2, g, Color.White, width = 3f)
            MapMarkers.Badge.Locked -> icon(ApgoIcons.Locked, c.x - g / 2, g, Color.White, width = 2.6f)
            else -> drawCircle(Color.White, radius = r * 0.35f, center = c)
        }
    }
}
