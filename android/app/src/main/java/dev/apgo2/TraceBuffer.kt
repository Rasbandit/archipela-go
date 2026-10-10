package dev.apgo2

import org.maplibre.android.geometry.LatLng
import uniffi.apgo_ffi.TraceDelta
import uniffi.apgo_ffi.TrackSegmentOut

/**
 * This session's matched trace as the map holds it, kept up to date from the engine's deltas (`Engine.traceMatchedSince`): settled
 * lines are appended, the provisional tail is replaced, and a reset (another game, a line thinned at its memory cap) replaces
 * everything. Not thread-safe: use it from the UI thread.
 */
internal class TraceBuffer {
    private val settled = mutableListOf<MutableList<LatLng>>()
    private var tail: List<LatLng> = emptyList()

    /** What to pass to the engine next; 0 asks for everything. */
    var cursor = 0UL
        private set

    private var fromMs: Long? = null
    private var held = false

    /** Apply [d]. True when the journal's trace before the session must be reloaded: a reset, or the session start is unknown or moved. */
    fun apply(d: TraceDelta): Boolean {
        val older = d.reset || !held || d.fromMs == null || d.fromMs != fromMs
        if (d.reset) settled.clear()
        val append = d.append.map { it.latLngs() }
        val last = settled.lastOrNull()
        val first = append.firstOrNull()
        val rest =
            if (d.joins && last != null && first != null) {
                last += first
                append.drop(1)
            } else {
                append
            }
        rest.forEach { settled += it.toMutableList() }
        tail = d.tail.latLngs()
        cursor = d.cursor
        fromMs = d.fromMs
        held = true
        return older
    }

    /** Forget everything (no game open). */
    fun clear() {
        settled.clear()
        tail = emptyList()
        cursor = 0UL
        fromMs = null
        held = false
    }

    /** The lines to draw, oldest first: the settled ones, then the tail. */
    fun lines(): List<List<LatLng>> = settled.map { it.toList() } + listOf(tail).filter { it.isNotEmpty() }

    private fun TrackSegmentOut.latLngs() = points.map { LatLng(it.lat, it.lon) }
}
