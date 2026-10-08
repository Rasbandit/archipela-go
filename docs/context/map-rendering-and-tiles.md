# Context Doc: Map Rendering, Tiles, Offline, Hosting Cost

_Last verified: 2026-10-07_

## Status

Research complete. **[P]** = checked against primary page this session (OSM tile policy, OpenFreeMap ToS). Rest is from Perplexity summaries; **[U]** = unverified.

## What This Is

Free options to draw a map in an Android-first app (MapLibre Native), serve or bundle tiles, support offline regions, and what hosting costs.

## Options

| Option | Cost / limits | Offline | Commercial / free-app OK? | Notes |
| --- | --- | --- | --- | --- |
| tile.openstreetmap.org (raster) | Free, best-effort, no SLA [P] | **Forbidden**: no prefetch, no "download area" [P] | Allowed with UA, attribution, caching, but may be blocked at any time [P] | Needs distinct UA (no default `okhttp`). Dev/prototype only |
| OpenFreeMap (public) | Free, no key, no stated view/request limit | ToS forbids "collect data from the service in automated ways without permission" [P]. Treat prefetch as not allowed; ask <info@openfreemap.org> | Yes, attribution "OpenFreeMap © OpenMapTiles Data from OpenStreetMap" | OpenMapTiles schema, MapLibre styles. May be discontinued without notice [P]. Best zero-effort online basemap |
| Protomaps PMTiles, self-hosted | Storage + requests only (R2: $0 egress) | Yes: `pmtiles extract --bbox` per region; MapLibre Android loads `pmtiles://file://` | Yes (ODbL data, attribution) | Planet ~120 GB z0-15; city extract ~few MB-100 MB. Hosted API (maps.protomaps.com) is non-commercial, ~1M tiles/mo soft limit, key needed |
| MapLibre Native Android | Open source (BSD) | Own offline pack API for raster/vector URL sources; **PMTiles sources do not use offline packs** | Yes | PMTiles support since Android 11.7.0 [U: verify against current release]; URL must be absolute |
| MapTiler Cloud Free | 5k sessions + 100k requests/mo, suspended when over | Not granted | **Non-commercial only** | Fine for hobby; not a plan for scale |
| Stadia Maps Free | 200k credits/mo | Offline needs paid subscription, max 100 MB/device | **Non-commercial only** | Same |
| Mapbox / Google Maps SDK | Free tiers, proprietary | Own rules | Yes with their terms | Data-use clauses block pairing with other data. Avoid |

## Recommendation

- v0: MapLibre + OpenFreeMap style (online only), attribution control on.
- v1: Self-hosted Protomaps extract per region (z0-14), served from R2 or bundled; style from `protomaps-themes-base`. No dependence on a donation-run service.
- Offline: **two** offline layers: (a) basemap region PMTiles file downloaded into app storage, (b) POI atlas cells (see `poi-atlas-and-server-options.md`).
- Keep tile URL/style URL remotely configurable (OSM policy also recommends not hardcoding).

## Offline region sizing (rough, [U] beyond cited examples)

| Region | PMTiles size |
| --- | --- |
| Small town, z0-14 | few MB - tens of MB |
| Large metro | tens - ~100 MB (Berlin cited ~71-84 MB) |
| Region at z15 | hundreds of MB |

Each extra zoom level roughly doubles size; use `pmtiles extract --dry-run` and `--maxzoom=14`. Game needs walkable map, so z0-14 plus overzoom is enough.

## Hosting static files (PMTiles, atlas cells)

| Host | Cost | Range requests | Fit |
| --- | --- | --- | --- |
| Cloudflare R2 + custom domain | $0.015/GB-mo, 10 GB free; Class A $4.50/M (1M free), Class B $0.36/M (10M free); **egress $0** | Yes (206) | Best. Put Cloudflare cache in front |
| GitHub Pages | Free; ~100 GB/mo soft bandwidth; 100 MB file limit in repo | Served fine but files capped | OK for small atlas cells (< 100 MB), not planet tiles. Soft limit risk if app goes viral |
| S3 (+CloudFront) | ~$0.09/GB egress after free 100 GB/mo | Yes | Egress risk; avoid for free app |
| Overture's own S3 | Free to read; not ours | Yes | Build-time only; do not point app at it |

Cost sketch: 100k MAU x 2 regions x 50 MB basemap = 10 TB egress per refresh. R2: ~$0 egress, ~$2 storage and operations. S3: ~$900. This is why R2 matters.

## Auth/attribution

No keys needed for OpenFreeMap or self-hosted PMTiles. Always show "© OpenStreetMap contributors" (MapLibre control does it). Overture/Foursquare/Wikimedia credits go in an About screen.

## Failed Approaches / Dead Ends

- Raster OSM tiles with a "download for offline" button: explicit policy violation [P].
- MapLibre offline pack manager with a `pmtiles://` source: not supported; use file-based extract instead.
- MapTiler/Stadia free tiers: commercial/offline restrictions make them dev-only.

## Gotchas

- `okhttp` default user-agent gets OSM tiles blocked [P].
- PMTiles basemap glyph/sprite assets are separate; bundle fonts (or host on R2) or labels vanish offline.
- Style must match tile schema (Protomaps schema != OpenMapTiles schema); do not mix styles and tiles.
- Wikimedia/OSM tools use anonymised UA logs; set a stable UA with contact.

## References

- <https://operations.osmfoundation.org/policies/tiles/>
- <https://openfreemap.org/tos/>
- <https://docs.protomaps.com/basemaps/downloads> , <https://docs.protomaps.com/pmtiles/cloud-storage>
- <https://maplibre.org/maplibre-native/android/examples/data/PMTiles/>
- <https://developers.cloudflare.com/r2/pricing/>
