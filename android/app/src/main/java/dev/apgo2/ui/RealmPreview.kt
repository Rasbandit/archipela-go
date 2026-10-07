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
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import java.io.File
import kotlin.coroutines.resume
import kotlin.math.PI
import kotlin.math.atan
import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.log2
import kotlin.math.max
import kotlin.math.tan
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

/** One find on a preview: where it is and the colour of its kind. */
data class PreviewDot(val lat: Double, val lon: Double, val color: Color)

/** Side of the square preview, in dp. The snapshot and the drawing over it use the same units. */
const val PREVIEW_DP = 104

/** Web-mercator, 0..1 across the world. The same maths as the map, so overlays line up with a snapshot of it. */
private fun mercX(lon: Double) = (lon + 180.0) / 360.0
private fun mercY(lat: Double) = (1.0 - ln(tan(PI / 4.0 + Math.toRadians(lat) / 2.0)) / PI) / 2.0
private fun latOfY(y: Double) = Math.toDegrees(2.0 * atan(exp(PI * (1.0 - 2.0 * y))) - PI / 2.0)

/** Where a preview of an outline looks: a centre and a zoom that fit the outline in the square with a margin. */
data class PreviewFrame(val center: LatLng, val zoom: Double) {
    /** Position of a point in the square, 0..1 on both axes. */
    fun at(lat: Double, lon: Double): Offset {
        val world = 512.0 * Math.pow(2.0, zoom) // map points across the world at this zoom
        val x = (mercX(lon) - mercX(center.longitude)) * world / PREVIEW_DP + 0.5
        val y = (mercY(lat) - mercY(center.latitude)) * world / PREVIEW_DP + 0.5
        return Offset(x.toFloat(), y.toFloat())
    }

    /** A name for the frame, so the same view is rendered once and reused (rounded: tiny differences are the same picture). */
    val key: String get() = "%.4f_%.4f_%.2f".format(center.latitude, center.longitude, zoom)
}

fun frameFor(outline: List<Pair<Double, Double>>): PreviewFrame? {
    if (outline.size < 3) return null
    val xs = outline.map { mercX(it.second) }
    val ys = outline.map { mercY(it.first) }
    val extent = max(max(xs.max() - xs.min(), ys.max() - ys.min()), 1e-9)
    val zoom = log2(PREVIEW_DP * 0.76 / (extent * 512.0)).coerceIn(2.0, 17.0)
    return PreviewFrame(LatLng(latOfY((ys.min() + ys.max()) / 2.0), ((xs.min() + xs.max()) / 2.0) * 360.0 - 180.0), zoom)
}

/** One snapshot is drawn at a time: they use the map engine, and a list of cards must not start a dozen at once. */
private val snapshotLock = Mutex()
private const val STYLE_URL = "https://tiles.openfreemap.org/styles/liberty"

/**
 * The map behind a realm preview: a saved picture if there is one, otherwise rendered now (needs the network once) and saved. Returns null when
 * it cannot be rendered (offline), in which case the card keeps its drawn background.
 */
suspend fun mapSnapshot(context: Context, frame: PreviewFrame): Bitmap? {
    val dir = File(context.filesDir, "previews")
    val file = File(dir, "map-${frame.key}.png")
    withContext(Dispatchers.IO) { if (file.exists()) BitmapFactory.decodeFile(file.path) else null }?.let { return it }
    val density = context.resources.displayMetrics.density
    val bitmap: Bitmap = snapshotLock.withLock {
        withContext(Dispatchers.Main) {
            MapLibre.getInstance(context)
            suspendCancellableCoroutine<Bitmap?> { cont ->
                val options = MapSnapshotter.Options((PREVIEW_DP * density).toInt(), (PREVIEW_DP * density).toInt())
                    .withStyleBuilder(Style.Builder().fromUri(STYLE_URL))
                    .withCameraPosition(CameraPosition.Builder().target(frame.center).zoom(frame.zoom).build())
                    .withLogo(false)
                val snapshotter = MapSnapshotter(context, options)
                cont.invokeOnCancellation { snapshotter.cancel() }
                snapshotter.start({ snap -> if (cont.isActive) cont.resume(snap.bitmap) }, { if (cont.isActive) cont.resume(null) })
            }
        }
    } ?: return null
    withContext(Dispatchers.IO) {
        runCatching { dir.mkdirs(); file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) } }
    }
    return bitmap
}

/**
 * A small preview of a realm: the map (when there is one) with the realm's outline and its finds as dots in their kind colours on top.
 * The dots and outline are drawn live, so marks and rescans show without drawing the map again. Without a map it is a plain schematic.
 */
@Composable
fun RealmPreview(outline: List<Pair<Double, Double>>, dots: List<PreviewDot>, map: Bitmap?, modifier: Modifier = Modifier) {
    val ink = MaterialTheme.colorScheme.primary
    val frameColor = MaterialTheme.colorScheme.outline
    val frame = frameFor(outline)
    Canvas(modifier.clip(RoundedCornerShape(12.dp)).background(MaterialTheme.colorScheme.surfaceVariant)) {
        if (frame == null) { drawRoundRect(frameColor, cornerRadius = androidx.compose.ui.geometry.CornerRadius(12.dp.toPx()), style = Stroke(width = 4.dp.toPx())); return@Canvas }
        map?.let { drawImage(it.asImageBitmap(), dstSize = androidx.compose.ui.unit.IntSize(size.width.toInt(), size.height.toInt())) }
        fun px(lat: Double, lon: Double) = frame.at(lat, lon).let { Offset(it.x * size.width, it.y * size.height) }
        val shape = Path().apply {
            outline.forEachIndexed { i, (lat, lon) -> px(lat, lon).let { if (i == 0) moveTo(it.x, it.y) else lineTo(it.x, it.y) } }
            close()
        }
        drawPath(shape, ink.copy(alpha = if (map == null) 0.12f else 0.16f))
        if (map != null) drawPath(shape, Color.White, style = Stroke(width = 4.dp.toPx())) // a halo, so the outline reads on any map
        drawPath(shape, ink, style = Stroke(width = 2.dp.toPx()))
        dots.forEach { d ->
            val c = px(d.lat, d.lon)
            drawCircle(d.color, radius = if (map != null) 1.6.dp.toPx() else 1.8.dp.toPx(), center = c)
        }
        // A border, so the picture does not melt into the card around it.
        drawRoundRect(frameColor, cornerRadius = androidx.compose.ui.geometry.CornerRadius(12.dp.toPx()), style = Stroke(width = 4.dp.toPx()))
    }
}

/** A preview of the home spot: the map around it with the house marker, and nothing else (no outline, no you). */
@Composable
fun HomePreview(map: Bitmap?, modifier: Modifier = Modifier) {
    val frameColor = MaterialTheme.colorScheme.outline
    val house = remember { renderMarker(ApgoIcons.Home, 120, ApgoPalette.home) }
    Canvas(modifier.clip(RoundedCornerShape(12.dp)).background(MaterialTheme.colorScheme.surfaceVariant)) {
        map?.let { drawImage(it.asImageBitmap(), dstSize = androidx.compose.ui.unit.IntSize(size.width.toInt(), size.height.toInt())) }
        val side = (size.minDimension * 0.46f).toInt()
        drawImage(
            house.asImageBitmap(), dstOffset = androidx.compose.ui.unit.IntOffset(((size.width - side) / 2).toInt(), ((size.height - side) / 2).toInt()),
            dstSize = androidx.compose.ui.unit.IntSize(side, side),
        )
        drawRoundRect(frameColor, cornerRadius = androidx.compose.ui.geometry.CornerRadius(12.dp.toPx()), style = Stroke(width = 4.dp.toPx()))
    }
}

/** The zoom a home preview is drawn at: a few blocks around the spot. */
const val HOME_PREVIEW_ZOOM = 15.2
