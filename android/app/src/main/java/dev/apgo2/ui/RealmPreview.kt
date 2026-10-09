package dev.apgo2.ui

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.maplibre.android.MapLibre
import org.maplibre.android.camera.CameraPosition
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.maps.Style
import org.maplibre.android.snapshotter.MapSnapshotter
import java.io.File
import kotlin.coroutines.resume
import kotlin.math.PI
import kotlin.math.atan
import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.log2
import kotlin.math.max
import kotlin.math.tan

/** One find on a preview: where it is and the colour of its kind. */
internal data class PreviewDot(
    val lat: Double,
    val lon: Double,
    val color: Color,
)

/** Side of the square preview, in dp. The snapshot and the drawing over it use the same units. */
internal const val PREVIEW_DP = 104

/** The zoom a home preview is drawn at: a few blocks around the spot. */
internal const val HOME_PREVIEW_ZOOM = 15.2

private const val STYLE_URL = "https://tiles.openfreemap.org/styles/liberty"
private const val WORLD_TILE_PX = 512.0
private const val QUARTER_PI = PI / 4.0
private const val HALF_TURN_DEG = 180.0
private const val FULL_TURN_DEG = 360.0
private const val MIN_OUTLINE_POINTS = 3
private const val MIN_ZOOM = 2.0
private const val MAX_ZOOM = 17.0
private const val OUTLINE_FILL_FRACTION = 0.76
private const val MIN_EXTENT = 1e-9
private const val PNG_QUALITY = 100
private const val HOUSE_PX = 120
private const val HOUSE_FRACTION = 0.46f
private const val CARD_CORNER_DP = 12
private const val BORDER_DP = 4
private const val OUTLINE_DP = 2
private const val FILL_ALPHA_WITH_MAP = 0.16f
private const val FILL_ALPHA_PLAIN = 0.12f
private const val DOT_DP_WITH_MAP = 1.6
private const val DOT_DP_PLAIN = 1.8

// Web-mercator, 0..1 across the world. The same maths as the map, so overlays line up with a snapshot of it.
private fun mercX(lon: Double) = (lon + HALF_TURN_DEG) / FULL_TURN_DEG

private fun mercY(lat: Double) = (1.0 - ln(tan(QUARTER_PI + Math.toRadians(lat) / 2.0)) / PI) / 2.0

private fun latOfY(y: Double) = Math.toDegrees(2.0 * atan(exp(PI * (1.0 - 2.0 * y))) - PI / 2.0)

/** Where a preview of an outline looks: a centre and a zoom that fit the outline in the square with a margin. */
internal data class PreviewFrame(
    val center: LatLng,
    val zoom: Double,
) {
    /** A name for the frame, so the same view is rendered once and reused (rounded: tiny differences are the same picture). */
    val key: String get() = "%.4f_%.4f_%.2f".format(center.latitude, center.longitude, zoom)
}

/** Position of a point in the square, 0..1 on both axes. */
internal fun PreviewFrame.pointAt(
    lat: Double,
    lon: Double,
): Offset {
    val world = WORLD_TILE_PX * Math.pow(2.0, zoom) // map points across the world at this zoom
    val x = (mercX(lon) - mercX(center.longitude)) * world / PREVIEW_DP + 0.5
    val y = (mercY(lat) - mercY(center.latitude)) * world / PREVIEW_DP + 0.5
    return Offset(x.toFloat(), y.toFloat())
}

/** The frame that fits [outline] (lat, lon pairs) in the square, or null when it is not a shape yet. */
internal fun frameFor(outline: List<Pair<Double, Double>>): PreviewFrame? {
    if (outline.size < MIN_OUTLINE_POINTS) return null
    val xs = outline.map { mercX(it.second) }
    val ys = outline.map { mercY(it.first) }
    val extent = max(max(xs.max() - xs.min(), ys.max() - ys.min()), MIN_EXTENT)
    val zoom = log2(PREVIEW_DP * OUTLINE_FILL_FRACTION / (extent * WORLD_TILE_PX)).coerceIn(MIN_ZOOM, MAX_ZOOM)
    val centerLon = (xs.min() + xs.max()) / 2.0 * FULL_TURN_DEG - HALF_TURN_DEG
    return PreviewFrame(LatLng(latOfY((ys.min() + ys.max()) / 2.0), centerLon), zoom)
}

// One snapshot is drawn at a time: they use the map engine, and a list of cards must not start a dozen at once.
private val snapshotLock = Mutex()

/**
 * The map behind a realm preview: a saved picture if there is one, otherwise rendered now (needs the network once) and saved.
 * Returns null when it cannot be rendered (offline), in which case the card keeps its drawn background.
 */
internal suspend fun mapSnapshot(
    context: Context,
    frame: PreviewFrame,
): Bitmap? {
    val file = File(File(context.filesDir, "previews"), "map-${frame.key}.png")
    val saved = withContext(Dispatchers.IO) { if (file.exists()) BitmapFactory.decodeFile(file.path) else null }
    return saved ?: renderSnapshot(context, frame)?.also { save(it, file) }
}

private suspend fun renderSnapshot(
    context: Context,
    frame: PreviewFrame,
): Bitmap? {
    val sidePx = (PREVIEW_DP * context.resources.displayMetrics.density).toInt()
    return snapshotLock.withLock {
        withContext(Dispatchers.Main) {
            MapLibre.getInstance(context)
            suspendCancellableCoroutine<Bitmap?> { cont ->
                val camera =
                    CameraPosition
                        .Builder()
                        .target(frame.center)
                        .zoom(frame.zoom)
                        .build()
                val options =
                    MapSnapshotter
                        .Options(sidePx, sidePx)
                        .withStyleBuilder(Style.Builder().fromUri(STYLE_URL))
                        .withCameraPosition(camera)
                        .withLogo(false)
                val snapshotter = MapSnapshotter(context, options)
                cont.invokeOnCancellation { snapshotter.cancel() }
                snapshotter.start({ snap -> if (cont.isActive) cont.resume(snap.bitmap) }, { if (cont.isActive) cont.resume(null) })
            }
        }
    }
}

private suspend fun save(
    bitmap: Bitmap,
    file: File,
) {
    withContext(Dispatchers.IO) {
        runCatching {
            file.parentFile?.mkdirs()
            file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
        }
    }
}

/**
 * A small preview of a realm: the map (when there is one) with the realm's outline and its finds as dots in their kind colours on top.
 * The dots and outline are drawn live, so marks and rescans show without drawing the map again. Without a map it is a plain schematic.
 */
@Composable
internal fun RealmPreview(
    outline: List<Pair<Double, Double>>,
    dots: List<PreviewDot>,
    map: Bitmap?,
    modifier: Modifier = Modifier,
) {
    val ink = MaterialTheme.colorScheme.primary
    val frameColor = MaterialTheme.colorScheme.outline
    val frame = frameFor(outline)
    Canvas(modifier.clip(RoundedCornerShape(CARD_CORNER_DP.dp)).background(MaterialTheme.colorScheme.surfaceVariant)) {
        if (frame != null) {
            map?.let { drawMap(it) }
            drawOutline(frame, outline, ink, map != null)
            dots.forEach { d ->
                val c = frame.pxAt(this, d.lat, d.lon)
                drawCircle(d.color, radius = (if (map != null) DOT_DP_WITH_MAP else DOT_DP_PLAIN).dp.toPx(), center = c)
            }
        }
        // A border, so the picture does not melt into the card around it.
        drawBorder(frameColor)
    }
}

/** A preview of the home spot: the map around it with the house marker, and nothing else (no outline, no you). */
@Composable
internal fun HomePreview(
    map: Bitmap?,
    modifier: Modifier = Modifier,
) {
    val frameColor = MaterialTheme.colorScheme.outline
    val house = remember { renderMarker(ApgoIcons.Home, HOUSE_PX, ApgoPalette.home) }
    Canvas(modifier.clip(RoundedCornerShape(CARD_CORNER_DP.dp)).background(MaterialTheme.colorScheme.surfaceVariant)) {
        map?.let { drawMap(it) }
        val side = (size.minDimension * HOUSE_FRACTION).toInt()
        drawImage(
            house.asImageBitmap(),
            dstOffset = IntOffset(((size.width - side) / 2).toInt(), ((size.height - side) / 2).toInt()),
            dstSize = IntSize(side, side),
        )
        drawBorder(frameColor)
    }
}

private fun DrawScope.drawMap(map: Bitmap) {
    drawImage(map.asImageBitmap(), dstSize = IntSize(size.width.toInt(), size.height.toInt()))
}

private fun DrawScope.drawBorder(color: Color) {
    drawRoundRect(color, cornerRadius = CornerRadius(CARD_CORNER_DP.dp.toPx()), style = Stroke(width = BORDER_DP.dp.toPx()))
}

private fun PreviewFrame.pxAt(
    scope: DrawScope,
    lat: Double,
    lon: Double,
) = pointAt(lat, lon).let { Offset(it.x * scope.size.width, it.y * scope.size.height) }

private fun DrawScope.drawOutline(
    frame: PreviewFrame,
    outline: List<Pair<Double, Double>>,
    ink: Color,
    onMap: Boolean,
) {
    val shape =
        Path().apply {
            outline.forEachIndexed { i, (lat, lon) ->
                frame.pxAt(this@drawOutline, lat, lon).let { if (i == 0) moveTo(it.x, it.y) else lineTo(it.x, it.y) }
            }
            close()
        }
    drawPath(shape, ink.copy(alpha = if (onMap) FILL_ALPHA_WITH_MAP else FILL_ALPHA_PLAIN))
    if (onMap) drawPath(shape, ApgoPalette.onMap, style = Stroke(width = BORDER_DP.dp.toPx())) // a halo, so the outline reads on any map
    drawPath(shape, ink, style = Stroke(width = OUTLINE_DP.dp.toPx()))
}
