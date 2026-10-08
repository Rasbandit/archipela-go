# Context Doc: Free POI Data Sources

_Last verified: 2026-10-07_

## Status

Research complete. Facts below came from Perplexity summaries; items tagged **[P]** were confirmed against a primary page this session (Overture 2026-09-23 release notes, Overpass "Commons" doc, OSM tile policy, OpenFreeMap ToS). Everything else is secondary and should be re-checked before it is relied on. **[U]** = unverified.

## What This Is

Where genuine real-world POIs (landmarks, parks, murals, statues, historic markers, cafes, viewpoints) can come from for free, and what each license/limit means for an app that stores and redistributes POIs.

## Background: how Pokemon GO did it

POIs were seeded from Ingress portals (player-submitted), now come from Niantic Wayfarer (player nominations, reviewed against criteria: permanent, physical, publicly accessible, safe). OSM/Google only fed the base map and terrain, not Stops. Lesson: the genuineness filter was human review plus clear criteria. We can approximate it with data-agreement scoring (see bottom) and later community votes.

## Comparison

| Source | License | Access | Limits | Freshness | Quality / fit | Caveats |
| --- | --- | --- | --- | --- | --- | --- |
| OSM via Overpass (public) | ODbL | HTTP query | ~10k req/day, ~1 GB/day guideline, slots, 429/504 [P]. Doc explicitly lists "app relying on public instances as backend" as problematic [P] | Minutes | Best for landmarks, art, viewpoints, trailheads, memorials | Not usable as a per-player backend. OK for CI/offline builds |
| OSM planet / Geofabrik | ODbL | PBF files; planet ~88 GB, `osmium tags-filter` | Geofabrik: per-region, no bulk mirroring; use planet for global | Planet weekly, Geofabrik daily | Same data, no rate limits | POI extract is a derivative database: must attribute and offer ODbL share-alike for that DB |
| Nominatim (public) | ODbL | HTTP | 1 req/s, UA required, no bulk, apps discouraged | Live | Reverse geocode only | Don't use for POI discovery |
| Overture Places | CDLA-Permissive-2.0 mostly; Foursquare-sourced rows Apache-2.0 (check `sources`) | GeoParquet on S3/Azure, DuckDB, `overturemaps` CLI, PMTiles | None stated; pay your own egress | Monthly (2026-09-23.1) [P] | 81.5M places [P]. `confidence` 0-1 = existence likelihood. Mostly commercial POIs (shops, services); weak on nature/art | Schema v2.0.0 removed `categories`; use `taxonomy`/`basic_category` [P]. Needs filtering by taxonomy+confidence. No OSM data in Places theme |
| Foursquare OS Places | Apache-2.0 | Hugging Face (gated), Places portal/Iceberg/Snowflake | Public S3 ended Oct 2025 for new releases | Monthly (Sep 2026: 109.4M POIs) | Commercial POIs; already merged into Overture | Redundant with Overture; skip |
| Wikidata | CC0 | SPARQL, JSON dump (~103 GB bz2) | WDQS 60 s timeout, 5 concurrent/IP, UA required, 429 | Live | Highest "notability"; P625 coords; images via P18 | Sparse outside notable things. Great as a score signal |
| Wikipedia GeoSearch | Text CC BY-SA 4.0 | MediaWiki API `list=geosearch` | radius 10-10000 m, 500 results; Wikimedia 2026 global rate limits (unauth 200 tier, UA with contact) | Live | Notable places only | Display text needs attribution. Good for in-app "fun fact" at runtime |
| Commons geotagged files | Per-file (CC BY/SA/CC0/PD) | `geosearch` ns=6 | same as above | Live | Photos of real places | Check each file license; store only IDs |
| OpenTripMap | DB ODbL; sub-fields from OSM/Wikidata/Wikipedia | REST key | Free quota **[U]** (~1 req/s reported) | Unknown | Pre-curated tourist POIs | One-person service, no status page; don't depend on it |
| Mapillary | Imagery CC BY-SA, logo attribution | Graph + vector tiles, token | 10k/min search, 50k/day tiles | Live | Detected map features (signs, hydrants), not landmarks | Low fit |
| GeoNames | CC BY 4.0 | Dumps (daily), web API | 10k credits/day, 1k/hour | Daily | Natural features, parks (L.PRK), peaks, populated places | Many obscure/duplicate spots; good for names of natural features |
| Who's On First | Mostly CC0, some per-source licenses | GitHub data | none | Slow, maintenance **[U]** | Admin boundaries, some venues | Not a POI source for us |
| USGS GNIS | Public domain | Bulk download | none | Periodic | US named natural + cultural features | US only; includes obscure names |
| NPS Data API / NRHP | US gov (public domain; check photos) | Free key; NRHP bulk GIS | 1,000 req/hr/key | Periodic | Parks, visitor centers, NRHP historic places | US only. Ideal "genuine" US seed |
| HMDB.org (historical markers) | Copyright, no open license found **[U]** | Website | n/a | n/a | Great markers | Do not scrape/copy. Get marker locations from OSM `historic=memorial/plaque` instead; link out |
| Waymarking / Geocaching | Restrictive ToU, partner API personal-use only | n/a | n/a | n/a | Fun but not licensed | Do not use |
| Google Places (New) | Proprietary | REST | Free caps per SKU: 10k Essentials / 5k Pro / 1k Enterprise per month (since 2025-03) | Live | Best commercial data | Store `place_id` only; lat/lng cache max 30 days; content not for use with non-Google maps. Not suitable |
| Mapbox Search/Geocoding | Proprietary | REST | 100k temporary req/mo free | Live | OK | Temporary results cannot be stored; use only with Mapbox map. Not suitable |
| HERE | Proprietary | REST | Freemium; quota **[U]** | Live | OK | Caching limits product-specific. Not suitable |
| Foursquare Places API | Proprietary | REST | From 2026-06-01: 500 free Pro calls/mo (older pages say 10k) | Live | OK | Use the open dataset instead |

## Recommended source stack (for a pre-built atlas)

1. OSM extract (tags: tourism=attraction|viewpoint|artwork|museum|gallery, historic=*, leisure=park|nature_reserve, natural=peak|waterfall|spring, amenity=library|fountain, highway=trailhead, man_made=lighthouse|tower).
2. Wikidata (P625 + sitelinks) for notability signal, images, descriptions.
3. Overture Places (confidence >= ~0.8 + chosen taxonomy) for cafes/shops/venues, plus Overture `base` land for parks if needed.
4. US: NPS + NRHP + GNIS as optional enrichment.
5. Runtime enrichment (optional, on tap only): Wikipedia GeoSearch extract.

## Scoring "genuineness" (build-time, per POI)

| Signal | Weight idea |
| --- | --- |
| OSM `wikidata`/`wikipedia` tag, or Wikidata item within ~50 m with matching name | +3 |
| Present in 2+ independent sources (OSM + Overture/Wikidata/NPS) within ~50 m and fuzzy-name match | +2 |
| Overture `confidence` >= 0.9 / >= 0.7 | +2 / +1 |
| Has name and tags `tourism`/`historic`/`leisure`/`natural` | +1 |
| Has `image`/`wikimedia_commons`/`website`/`opening_hours` | +1 |
| OSM object last edited within ~2 years (`timestamp`), `check_date` | +1 |
| Penalties: no name, `disused:*`/`abandoned:*`/`lifecycle` prefixes, Overture `operating_status` closed, private access, schools/kindergartens, residential | -3 to exclude |

Output a 0-10 score plus `kind`, so the client can filter by difficulty/tier. Treat OSM `timestamp` as noisy (a typo fix refreshes it).

## Licensing implications

- OSM-derived POI file = ODbL derivative database. Publish it openly (we are doing that anyway on a CDN) and attribute "© OpenStreetMap contributors".
- Overture rows carry source-specific notices (Meta/Microsoft CDLA, Foursquare Apache). Keep `sources` provenance in the atlas and an ATTRIBUTION file.
- Mixing OSM and Overture rows in one database makes the combined DB ODbL-governed; keep a `source` column so layers can be separated.
- Wikidata CC0 is free; Wikipedia text/Commons images need per-item attribution, so store only IDs/titles and fetch live.

## Failed Approaches / Dead Ends

- One Overpass request per candidate (upstream): hits the 429/slot limit, and "asking for elements one by one" is named as abuse.
- Rotating Overpass mirrors to multiply quota: Commons doc says the two backends rate-limit independently but discourages this; third-party mirrors have their own undisclosed policies.
- Commercial POI APIs: storage/caching and display-with-their-map clauses conflict with a pre-built offline atlas.

## Gotchas

- Overture `categories` no longer exists as of v2.0.0 (2026-09-23); old tutorials break.
- Overture Places is dense in commercial POIs, thin in murals/statues/viewpoints; OSM and Wikidata cover those.
- Foursquare OS Places is already inside Overture Places.
- "HMDB" web results mostly describe a different (human metabolome) database; the marker site is hmdb.org.

## References

- Overture release notes: <https://docs.overturemaps.org/blog/2026/09/23/release-notes/>
- Overpass commons: <https://dev.overpass-api.de/overpass-doc/en/preface/commons.html>
- OSM tile policy: <https://operations.osmfoundation.org/policies/tiles/>
- Google Maps service terms: <https://cloud.google.com/maps-platform/terms/maps-service-terms>
- Related: `docs/context/archipela-go-location-generation.md`, `docs/context/archipela-go-upstream-architecture.md`
