package dev.apgo2.ui

import android.graphics.Bitmap
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
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
import kotlin.math.acos

/*
 * MapLibre symbols are bitmaps, so the Lucide icons the UI uses are drawn into bitmaps here. Lucide icons are stroked paths on a
 * 24x24 grid.
 */

private fun DrawScope.strokeIcon(
    group: VectorGroup,
    color: Color,
    width: Float = 2f,
) {
    group.forEach { node ->
        when (node) {
            is VectorPath -> {
                drawPath(
                    node.pathData.toPath(),
                    color,
                    style = Stroke(width = width, cap = StrokeCap.Round, join = StrokeJoin.Round),
                )
            }

            is VectorGroup -> {
                strokeIcon(node, color, width)
            }
        }
    }
}

private fun render(
    sizePx: Int,
    heightPx: Int = sizePx,
    draw: DrawScope.() -> Unit,
): Bitmap {
    val image = ImageBitmap(sizePx, heightPx)
    CanvasDrawScope().draw(Density(1f), LayoutDirection.Ltr, Canvas(image), Size(sizePx.toFloat(), heightPx.toFloat()), draw)
    return image.asAndroidBitmap()
}

private fun DrawScope.icon(
    icon: ImageVector,
    at: Float,
    sizePx: Float,
    color: Color,
    width: Float = 2f,
    top: Float = at,
) {
    withTransform({
        translate(left = at, top = top)
        scale(sizePx / icon.viewportWidth, sizePx / icon.viewportHeight, pivot = Offset.Zero)
    }) { strokeIcon(icon.root, color, width) }
}

// A map-pin shape: a round head of radius [r] centred on ([cx], [cy]) narrowing to a point at ([cx], [tipY]). The sides are the
// tangents from the tip to the head.
private fun DrawScope.teardrop(
    cx: Float,
    cy: Float,
    r: Float,
    tipY: Float,
    color: Color,
) {
    val side = Math.toDegrees(acos(r / (tipY - cy)).toDouble()).toFloat() // from straight down to where a side touches the head
    val shape =
        Path().apply {
            arcTo(Rect(cx - r, cy - r, cx + r, cy + r), DOWN + side, FULL_TURN - 2 * side, forceMoveTo = true)
            lineTo(cx, tipY)
            close()
        }
    drawPath(shape, color)
}

// A teardrop with a [ring]-wide border: the border shape, then the fill shape inset by [ring] (its tip moves up by more, since
// the sides slope).
private fun DrawScope.pinBody(
    cx: Float,
    cy: Float,
    r: Float,
    tipY: Float,
    ring: Float,
    border: Color,
    fill: Color,
) {
    teardrop(cx, cy, r, tipY, border)
    teardrop(cx, cy, r - ring, tipY - ring * (tipY - cy) / r, fill)
}

private const val DOWN = 90f
private const val FULL_TURN = 360f
private const val PIN_RING_FRACTION = 0.07f

// The difficulty dots' top row, below the head's centre in head widths: just under the head, in the widest part of the point.
private const val PIP_ROW_FRACTION = 0.52f

/**
 * A find on the map: a map pin (a round head with the icon, narrowing to a point below it) [sizePx] wide and
 * [MapMarkers.PIN_HEIGHT_RATIO] times as tall. The point is the spot: the map anchors the image at its bottom centre.
 */
internal fun renderFindPin(
    icon: ImageVector,
    sizePx: Int,
    fill: Color,
    ring: Color = ApgoPalette.onPin,
    ringFraction: Float = 0.07f,
): Bitmap {
    val s = sizePx.toFloat()
    val h = s * MapMarkers.PIN_HEIGHT_RATIO
    return render(sizePx, h.toInt()) {
        pinBody(s / 2, s / 2, s / 2, h, s * ringFraction, ring, fill)
        val inner = s * 0.54f
        icon(icon, (s - inner) / 2, inner, ApgoPalette.onPin)
    }
}

/** A round badge with the icon inside (you, on the map). */
internal fun renderPin(
    icon: ImageVector,
    sizePx: Int,
    fill: Color,
    glyph: Color = ApgoPalette.onPin,
    ring: Color = ApgoPalette.onPin,
    ringFraction: Float = 0.07f,
): Bitmap =
    render(sizePx) {
        val s = sizePx.toFloat()
        drawCircle(ring, radius = s / 2)
        drawCircle(fill, radius = s / 2 - s * ringFraction)
        val inner = s * 0.54f
        icon(icon, (s - inner) / 2, inner, glyph)
    }

/**
 * A cluster: a white disc (the count is drawn over it by the map) inside a ring of [segments], each a colour and its sweep in
 * degrees, clockwise from the top. Segments are split by a thin white gap so neighbouring colours stay distinct.
 */
internal fun renderRing(
    segments: List<Pair<Color, Float>>,
    sizePx: Int,
): Bitmap =
    render(sizePx) {
        val s = sizePx.toFloat()
        val band = s * 0.2f
        val gap = if (segments.size > 1) 4f else 0f
        drawCircle(ApgoPalette.onMap, radius = s / 2)
        val arc = Size(s - band - 4f, s - band - 4f)
        val topLeft = Offset((s - arc.width) / 2, (s - arc.height) / 2)
        var start = -90f
        segments.forEach { (color, sweep) ->
            drawArc(color, start + gap / 2, sweep - gap, useCenter = false, topLeft = topLeft, size = arc, style = Stroke(width = band))
            start += sweep
        }
    }

/** Just the icon, for drawing a glyph on top of another marker. */
internal fun renderGlyph(
    icon: ImageVector,
    sizePx: Int,
    color: Color = ApgoPalette.onPin,
): Bitmap = render(sizePx) { icon(icon, 0f, sizePx.toFloat(), color) }

/**
 * The icon itself, bold and with a light outline so it reads on any map: a marker that is the shape (a house) rather than a
 * badge around it.
 */
internal fun renderMarker(
    icon: ImageVector,
    sizePx: Int,
    color: Color,
    outline: Color = ApgoPalette.onMap,
): Bitmap =
    render(sizePx) {
        val s = sizePx.toFloat()
        val inset = s * 0.1f
        icon(icon, inset, s - 2 * inset, outline, width = 4.6f)
        icon(icon, inset, s - 2 * inset, color, width = 2.4f)
    }

/**
 * A quest pin: the find pin's shape, its body in the state colour, its kind's icon with 1 to 3 dots under it for difficulty, and a
 * badge on the lower right of the head: an amber dot in progress, a lock when locked. Done needs none (the pin is green).
 */
internal fun renderQuestPin(
    icon: ImageVector,
    sizePx: Int,
    fill: Color,
    badge: MapMarkers.Badge,
    pips: Int,
): Bitmap {
    val s = sizePx.toFloat()
    val h = s * MapMarkers.PIN_HEIGHT_RATIO
    return render(sizePx, h.toInt()) {
        val body = s * 0.86f // leave room for the badge beside the head
        val head = Offset(s / 2, body / 2)
        pinBody(head.x, head.y, body / 2, h, body * PIN_RING_FRACTION, ApgoPalette.onPin, fill)
        // The icon fills the head; the difficulty dots sit in the point below it (three make a triangle pointing down).
        val inner = body * 0.64f
        icon(icon, head.x - inner / 2, inner, ApgoPalette.onPin, top = head.y - inner / 2)
        val dot = body * 0.05f
        PipLayout.centers(pips, head.x, head.y + body * PIP_ROW_FRACTION, gap = body * 0.145f).forEach { (x, y) ->
            drawCircle(ApgoPalette.onPin, radius = dot, center = Offset(x, y))
        }
        if (badge != MapMarkers.Badge.None) {
            val r = s * 0.2f
            val c = Offset(s - r, body - r)
            drawCircle(ApgoPalette.onPin, radius = r, center = c)
            val locked = badge == MapMarkers.Badge.Locked
            drawCircle(if (locked) ApgoPalette.questLocked else ApgoPalette.questProgress, radius = r - s * 0.025f, center = c)
            val g = r * 1.15f
            if (locked) {
                icon(ApgoIcons.Locked, c.x - g / 2, g, ApgoPalette.onPin, width = 2.6f, top = c.y - g / 2)
            } else {
                drawCircle(ApgoPalette.onPin, radius = r * 0.35f, center = c)
            }
        }
    }
}
