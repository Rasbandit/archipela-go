package dev.apgo2

import android.graphics.PointF
import android.graphics.RectF
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import dev.apgo2.ui.ApgoIcons
import dev.apgo2.ui.ApgoPalette
import dev.apgo2.ui.Help
import dev.apgo2.ui.hex
import org.maplibre.android.camera.CameraUpdateFactory
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.geometry.LatLngBounds
import org.maplibre.android.maps.MapLibreMapOptions
import org.maplibre.android.maps.MapView
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.CircleLayer
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory.circleColor
import org.maplibre.android.style.layers.PropertyFactory.circleRadius
import org.maplibre.android.style.layers.PropertyFactory.circleStrokeColor
import org.maplibre.android.style.layers.PropertyFactory.circleStrokeWidth
import org.maplibre.android.style.layers.PropertyFactory.textAllowOverlap
import org.maplibre.android.style.layers.PropertyFactory.textColor
import org.maplibre.android.style.layers.PropertyFactory.textField
import org.maplibre.android.style.layers.PropertyFactory.textFont
import org.maplibre.android.style.layers.PropertyFactory.textIgnorePlacement
import org.maplibre.android.style.layers.PropertyFactory.textSize
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.android.style.sources.GeoJsonSource
import uniffi.apgo_ffi.GeoPoint
import uniffi.apgo_ffi.RecapOut
import uniffi.apgo_ffi.RecapQuestOut

/** "Welcome home": how the last walk went, shown over the app until dismissed. */
@Composable
internal fun RecapScreen(
    r: RecapOut,
    onDone: () -> Unit,
) {
    BackHandler(onBack = onDone)
    var selected by rememberSaveable { mutableStateOf<Int?>(null) }
    val toggle = { i: Int -> selected = if (selected == i) null else i }
    Surface(Modifier.fillMaxSize()) {
        LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { Text(Help.RECAP_TITLE, style = MaterialTheme.typography.headlineMedium) }
            item { Text(RecapFormat.subtitle(r)) }
            if (r.route.isNotEmpty()) item { RecapMap(r.route, r.quests, selected, toggle) }
            items(RecapFormat.sections(r), key = { it.first }) { (label, lines) ->
                Text(label, style = MaterialTheme.typography.titleSmall)
                lines.forEach { Text(it) }
            }
            if (r.quests.isNotEmpty()) item { QuestsCard(r.quests, selected, toggle) }
            item { Button(onClick = onDone, modifier = Modifier.fillMaxWidth()) { Text(Help.RECAP_DONE) } }
        }
    }
}

private const val MAP_HEIGHT_DP = 200
private const val MAP_CORNER_DP = 12
private const val MAP_PADDING_PX = 48
private const val PIN_TAP_DP = 12f
private const val PIN_SOURCE = "recap-pins"
private const val PIN_LAYER = "recap-pins-dot"
private const val PIN_LABEL_LAYER = "recap-pins-n"
private const val PIN_RADIUS = 12f
private const val PIN_RADIUS_SELECTED = 15f
private const val PIN_STROKE = 2f
private const val PIN_TEXT_SIZE = 12f
private const val BADGE_DP = 28
private const val ROW_MIN_DP = 56
private const val CHEVRON_OPEN_DEG = 180f
private const val ROW_GAP_DP = 12

private fun tapArea(
    at: PointF,
    density: Float,
): RectF = (PIN_TAP_DP * density).let { RectF(at.x - it, at.y - it, at.x + it, at.y + it) }

private fun pinData(
    quests: List<RecapQuestOut>,
    selected: Int?,
) = GeoJson.collection(RecapFeatures.questPins(quests, selected))

private fun pinLayers() =
    listOf(
        CircleLayer(PIN_LAYER, PIN_SOURCE).withProperties(
            circleRadius(
                Expression.switchCase(
                    Expression.eq(Expression.get(MapProp.SELECTED), Expression.literal(true)),
                    Expression.literal(PIN_RADIUS_SELECTED),
                    Expression.literal(PIN_RADIUS),
                ),
            ),
            circleColor(ApgoPalette.questDone.hex()),
            circleStrokeColor(ApgoPalette.onMap.hex()),
            circleStrokeWidth(PIN_STROKE),
        ),
        SymbolLayer(PIN_LABEL_LAYER, PIN_SOURCE).withProperties(
            textField(Expression.toString(Expression.get("n"))),
            textFont(arrayOf(BOLD_FONT)),
            textSize(PIN_TEXT_SIZE),
            textColor(ApgoPalette.onBrand.hex()),
            textAllowOverlap(true),
            textIgnorePlacement(true),
        ),
    )

// The walk on a still map: the route drawn by the map's own trace layer, a numbered pin per placed quest, the camera fitted to the
// route. No gestures, so the list scrolls. Pin changes update the pin source's data; the map is never rebuilt.
@Composable
private fun RecapMap(
    route: List<List<GeoPoint>>,
    quests: List<RecapQuestOut>,
    selected: Int?,
    onPin: (Int) -> Unit,
) {
    val context = LocalContext.current
    val density = LocalDensity.current.density
    val latestPin by rememberUpdatedState(onPin)
    val styleReady = remember { mutableStateOf<Style?>(null) }
    val view =
        remember {
            MapStyle.init(context)
            val options = MapLibreMapOptions.createFromAttributes(context).logoEnabled(false).attributionEnabled(false)
            MapView(context, options).also { v ->
                v.getMapAsync { map ->
                    map.uiSettings.setAllGesturesEnabled(false)
                    map.addOnMapClickListener { ll ->
                        val area = tapArea(map.projection.toScreenLocation(ll), density)
                        map
                            .queryRenderedFeatures(area, PIN_LAYER)
                            .firstOrNull()
                            ?.getNumberProperty("n")
                            ?.let {
                                latestPin(it.toInt() - 1)
                                true
                            } == true
                    }
                    map.setStyle(Style.Builder().fromUri(MapStyle.URL)) { style ->
                        MapStyle.install(style)
                        style.getSourceAs<GeoJsonSource>(MapSource.TRACE)?.setGeoJson(GeoJson.collection(RecapFeatures.route(route)))
                        style.addSource(GeoJsonSource(PIN_SOURCE))
                        pinLayers().forEach(style::addLayer)
                        val points = route.flatten().map { LatLng(it.lat, it.lon) }
                        if (points.size >= 2) {
                            map.moveCamera(CameraUpdateFactory.newLatLngBounds(LatLngBounds.fromLatLngs(points), MAP_PADDING_PX))
                        }
                        styleReady.value = style
                    }
                }
            }
        }
    LaunchedEffect(styleReady.value, quests, selected) {
        styleReady.value?.getSourceAs<GeoJsonSource>(PIN_SOURCE)?.setGeoJson(pinData(quests, selected))
    }
    MapLifecycle(view, onShow = true)
    Box(Modifier.fillMaxWidth().height(MAP_HEIGHT_DP.dp).clip(RoundedCornerShape(MAP_CORNER_DP.dp))) {
        AndroidView({ view }, Modifier.fillMaxSize())
        Text(
            MapStyle.CREDIT,
            Modifier.align(Alignment.BottomEnd).background(MaterialTheme.colorScheme.surface).padding(horizontal = 4.dp),
            style = MaterialTheme.typography.labelSmall,
        )
    }
}

// The quests finished on the walk; a row opens to show its goal, when it was done and its reward.
@Composable
private fun QuestsCard(
    quests: List<RecapQuestOut>,
    selected: Int?,
    onToggle: (Int) -> Unit,
) {
    if (quests.isEmpty()) return
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(vertical = 8.dp)) {
            Text(Help.RECAP_QUESTS, Modifier.padding(horizontal = 16.dp, vertical = 8.dp), style = MaterialTheme.typography.titleMedium)
            quests.forEachIndexed { i, q -> QuestRow(i, q, open = selected == i) { onToggle(i) } }
        }
    }
}

@Composable
private fun QuestRow(
    index: Int,
    q: RecapQuestOut,
    open: Boolean,
    onClick: () -> Unit,
) {
    val turn by animateFloatAsState(if (open) CHEVRON_OPEN_DEG else 0f, label = "chevron")
    val time = RecapFormat.questTime(q)
    Column(
        Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { stateDescription = if (open) Help.EXPANDED else Help.COLLAPSED }
            .heightIn(min = ROW_MIN_DP.dp)
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(ROW_GAP_DP.dp)) {
            Box(Modifier.size(BADGE_DP.dp).background(ApgoPalette.questDone, CircleShape), contentAlignment = Alignment.Center) {
                Text("${index + 1}", color = ApgoPalette.onBrand, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.Bold)
            }
            Column(Modifier.weight(1f)) {
                Text(q.name, style = MaterialTheme.typography.bodyLarge)
                Text(
                    "${q.place} \u00b7 $time",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Icon(ApgoIcons.Expand, contentDescription = null, modifier = Modifier.rotate(turn))
        }
        AnimatedVisibility(open) {
            Column(Modifier.padding(start = (BADGE_DP + ROW_GAP_DP).dp, top = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                QuestDetail(Help.RECAP_GOAL, q.goal)
                QuestDetail(Help.RECAP_DONE_AT, time)
                q.reward?.let { QuestDetail(Help.RECAP_REWARD, it) }
            }
        }
    }
}

@Composable
private fun QuestDetail(
    label: String,
    value: String,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(ROW_GAP_DP.dp)) {
        Text(label, Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodyMedium)
        Text(value, Modifier.weight(2f), style = MaterialTheme.typography.bodyMedium)
    }
}
