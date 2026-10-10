# Context Doc: location libraries and industry practice

_Last verified: 2026-10-09 (crates.io and GitHub facts as of that day)_

Asked by the owner while building the location stack ("are we reinventing the wheel?"). Scope: `core/src/loc/` (IMM, matcher,
graph, heading), `core/src/geo.rs` (Douglas-Peucker, polyline) and the Android request. How the stack works:
`location-estimation.md`. Licence rule for linked code: nothing GPL/AGPL/LGPL; MIT/Apache/BSD/zlib are fine (the owner also
accepted Google's proprietary Play services SDK licence, see Outcome).

## Outcome (what was done with this research)

| Item | Result |
| --- | --- |
| Fused Orientation Provider | Adopted (#114): compass from Play services while the map is on screen, `headingErrorDegrees` as `HeadingIn.error_deg`; rotation vector off screen and without Play services (`Compass.kt`, `Sensors.kt`) |
| Play services location | Adopted (owner, 2026-10-09, "platform first"): `FusedLocationProviderClient` on every Android version with Play services (`GpsPolicy.providers`, `gmsRequest`) |
| Douglas-Peucker | Swapped to `geo::SimplifyIdx` (`geo` 0.33, `default-features = false`; +2.4 kB on the arm64 `.so`); our pinned-vertex split stays (`core/src/geo.rs` `simplify_pinned`) |
| Encoded polyline | Kept ours: the `polyline` 0.11 crate decodes a cut-short string without an error |
| IMM, matcher, street graph, bridge, carry offset, step calibration | Kept ours (verdicts below) |
| Activity Recognition prior | Not done (optional later) |

## Short answer

- **The architecture is the textbook one.** Run the OS fused provider, then an app-side filter, then HMM map matching (Newson and
  Krumm 2009) for display. Every open matcher (OSRM, Valhalla Meili, GraphHopper, barefoot, FMM) uses that same HMM/Viterbi model.
  IMM with CV models is the standard multi-motion-mode tracker.
- **No library does most of this for us on the phone, offline, in Rust, under a permissive licence.** The open map matchers are
  server engines in C++/Java. The one Rust map-matching stack (`routers`) is GPL-3.0. No Rust IMM crate is maintained.
- **Where a library would have saved a little:** Douglas-Peucker, polyline encoding, and possibly the grid index (`geo`, `polyline`,
  `rstar`). That is roughly a day of work, and our versions are already written and tested. Swapping now saves little.
- **The one real quick win is on the platform side:** Google's **Fused Orientation Provider** (FOP). It gives a fused heading plus a
  heading error estimate, and our compass gating needs exactly that error estimate. Activity Recognition is a cheap extra prior
  for the IMM.

## A. What the big apps do

| App | On-device smoothing | Map matching | Source |
| --- | --- | --- | --- |
| Google Maps | Uses the Play services Fused Location Provider (FLP). FLP fuses GNSS, Wi-Fi, cell and sensors. Since 2020 it also applies 3D-mapping-aided GNSS corrections in 3,850+ cities, with dual-frequency L1/L5 where the chip has it. The app's own smoothing is not published. | Yes for driving navigation (proprietary). For walking no snapping is documented: the blue dot shows a heading cone from fused orientation. | [Android blog: Improving urban GPS accuracy](https://android-developers.googleblog.com/2020/12/improving-urban-gps-accuracy-for-your.html), [FOP announcement](https://android-developers.googleblog.com/2024/03/introducing-fused-orientation-provider-api.html) |
| Pokémon GO / Niantic | Takes the platform location (FLP / Core Location) for the 2D map. Niantic has published no GPS filter. | None. Players are not snapped to streets. Precise placement uses VPS (camera, server-side anchors), which is a separate system. | [Niantic VPS / Playgrounds](https://nianticlabs.com/news/pokemon-playgrounds?hl=en) |
| Strava | Records OS locations and rejects bad points. It publishes nothing about an on-device Kalman filter. | Server-side, after upload: segment matching, and "Slide", which iteratively pulls lines toward its global GPS heatmap. Crowd data, not phone sensors. | [Strava Slide](https://labs.strava.com/slide/), [SOTM 2014 talk](https://labs.strava.com/slide/slide-SOTM-2014.pdf), [Bad GPS data](https://support.strava.com/en-us/articles/15402181-bad-gps-data) |
| Apple Maps | Core Location. Fusion is system-level and not documented. | Core Location itself is widely observed to road-snap with `.automotiveNavigation`. It is not documented as a contract; `.otherNavigation` and `.fitness` are not snapped. | [CLActivityType](https://developer.apple.com/documentation/corelocation/clactivitytype), [SO: snap to road](https://stackoverflow.com/questions/19815248/snap-to-road-for-apple-maps), [regex.info report](https://regex.info/blog/2015-12-03/2651) |

Takeaway: the big apps rely on the OS provider for the fix, and either do no pedestrian matching (Google walking, Niantic) or do it
on a server (Strava, Google driving). **Live on-device pedestrian matching to OSM paths is unusual.** That is a product choice of
ours, not an industry default. It is justified because the map shows streets from our own scans and there is no server.

## B. What the OS gives us, and what it does not

| Need | Android | iOS | Do we still need ours? |
| --- | --- | --- | --- |
| Multi-source fix fusion (GNSS + Wi-Fi + cell + sensors) | Play services FLP does it. The algorithm is unpublished, and Google promises neither Kalman nor PDR. AOSP `fused` mostly just picks between `gps` and `network`. GrapheneOS and other no-GMS phones get raw `gps`. | Core Location does it (unpublished). | Yes. FLP output still jumps and drifts when you stand still, and no-GMS phones get no fusion at all. |
| Urban-canyon correction | FLP: 3D-mapping-aided GNSS (no app API, automatic) | Apple does something similar in some cities (unpublished) | No. We get it for free by using FLP. |
| Stationary hold, quest-grade gating, outlier relocation, odometer | No | Partly: Core Location pauses updates when you are still (`CLLocationUpdate` diagnostics) | **Yes.** Quest logic needs a covariance and a gate, which no OS gives. |
| Map matching for walking | No (FLP does not snap) | No (only automotive snapping is observed) | **Yes, if we want a snapped display.** |
| Device heading | `TYPE_ROTATION_VECTOR`, or **FOP** (Play services 2024+): quaternion, heading, **heading error in degrees**, true north when declination is known | `CLHeading` with `headingAccuracy` | Use FOP when Play services is present. Keep the rotation vector as the fallback. |
| Walking direction vs phone direction (carry offset) | No | No (`course` is from GPS movement only) | **Yes.** No API estimates pocket offset. |
| Motion mode | Activity Recognition Transition API (STILL / WALKING / RUNNING / ON_BICYCLE / IN_VEHICLE). Latency varies by device, from seconds upward. | `CMMotionActivityManager` (ordinal confidence) | Optional soft prior for the IMM mode probabilities. Never a hard switch. |
| Steps | `TYPE_STEP_COUNTER` / `TYPE_STEP_DETECTOR` | `CMPedometer` (steps, cadence, distance) | Platform sensors are right. Step length and PDR stay ours. |

Sources: [FLP overview](https://developers.google.com/location-context/fused-location-provider),
[FusedOrientationProviderClient](https://developers.google.com/android/reference/com/google/android/gms/location/FusedOrientationProviderClient),
[DeviceOrientation](https://developers.google.com/android/reference/com/google/android/gms/location/DeviceOrientation),
[Activity transitions](https://developer.android.com/develop/sensors-and-location/location/transitions),
[ActivityRecognitionClient](https://developers.google.com/android/reference/com/google/android/gms/location/ActivityRecognitionClient),
[CLLocationUpdate](https://developer.apple.com/documentation/corelocation/cllocationupdate),
[WWDC23 10180](https://developer.apple.com/videos/play/wwdc2023/10180/),
[CMMotionActivity](https://developer.apple.com/documentation/coremotion/cmmotionactivity),
[AOSP location internals](https://aospbooks.github.io/aosp-internal-book/33-location/).

## C. Library candidates

Facts come from the crates.io API and the GitHub API on 2026-10-09.

### Kalman / IMM (Rust)

| Crate | Licence | Latest / updated | Downloads | Fit |
| --- | --- | --- | --- | --- |
| [`adskalman`](https://github.com/strawlab/adskalman-rs) | MIT/Apache-2.0 | 0.18.0, 2026-08 | 257 k | Linear KF + RTS smoother, nalgebra, `no_std`. No IMM, no gating policy. |
| [`kfilter`](https://docs.rs/kfilter) | MIT | 0.5.1, 2026-05 | 21 k | KF/EKF/UKF over static `SMatrix`. No IMM. |
| [`kalman_filters`](https://github.com/destenson/kalman-filter-rs) | MIT | 1.0.1, 2025-10 | 92 k | KF/EKF/UKF/information filters, advertises a particle filter. No IMM. |
| [`filter`](https://lib.rs/crates/filter) (filter-rs) | MIT | 0.2.0, **2020-06** | 5 k | Lists an IMM, but has been dormant for 6 years. Do not adopt. |
| [`nalgebra`](https://github.com/dimforge/nalgebra) | Apache-2.0 | 0.35.0, 2026-05 | 94 M | Linear algebra only. |

Verdict: the Kalman update itself is about 50 lines in `mat.rs`. The value is in the IMM mixing, the gate, the relocation and the
stationary hold, and none of the crates has those. A crate would have saved at most a day. Adding `nalgebra` for 2x2 / 4x4 matrices
costs compile time and binary size for nothing. **Keep ours.**

### Map matching

| Project | Licence | Lang | Stars / last push | On-device? |
| --- | --- | --- | --- | --- |
| [Valhalla Meili](https://valhalla.github.io/valhalla/meili/) | MIT | C++ | 6.3 k / 2026-10 | Heavy: needs Valhalla tiles, Boost, protobuf. Server or local service. It has an online mode. |
| [OSRM `match`](https://github.com/Project-OSRM/osrm-backend) | BSD-2 | C++ | 8.1 k / 2026-10 | Server. Heavy preprocessing (CH/MLD). |
| [GraphHopper map matching](https://github.com/graphhopper/graphhopper) | Apache-2.0 | Java | 6.7 k / 2026-10 | JVM. Android is possible but heavy, and it gives iOS nothing. |
| [barefoot](https://github.com/bmwcarit/barefoot) | Apache-2.0 | Java | 693 / **2023-04** | Online HMM matcher, but dormant and Java. |
| [FMM](https://github.com/cyang-kth/fmm) | Apache-2.0 | C++ | 1.06 k / 2024-07 | Focused and fast, but uses a precomputed UBODT, expects batch input, and pulls GDAL/Boost. |
| [`routers`](https://github.com/routers-org/routers) (+ `routers_transition`) | **GPL-3.0** | Rust | young | Ruled out by licence. |
| [`pelorus`](https://crates.io/crates/pelorus) | MIT | Rust | 366 downloads, one release 2025-09 | Too immature to depend on. |
| [`hmmmm`](https://github.com/dangreco/hmmmm) | MIT | Rust | 4 k downloads, 2022 | Generic HMM only. Candidates and transitions are still ours. |
| [Mapbox Navigation SDK](https://docs.mapbox.com/android/navigation/guides/) | Proprietary ToS, billed per MAU and trip | Kotlin/C++ | | Free-drive map matching, tuned for roads and driving, with billing. No. |
| [MapLibre Navigation Android](https://github.com/maplibre/maplibre-navigation-android) | MIT | Java | 208 / 2026-09 | Snaps to an active route only, not free-roam. Android only. |
| [Ferrostar](https://github.com/stadiamaps/ferrostar) | BSD-3 | Rust core + UniFFI | 436 / 2026-10 | Same architecture as ours (Rust + UniFFI), but only snaps to a supplied route. A good code reference, not a replacement. |
| [Google Roads API Snap to Roads](https://developers.google.com/maps/documentation/roads/snap) | Google Maps Platform ToS, paid | server | | Online, billed, and display restrictions conflict with an OSM/MapLibre map. No. |

Comparative survey with licences and languages:
[Transactions in GIS 2023, tgis.13107](https://onlinelibrary.wiley.com/doi/full/10.1111/tgis.13107).

Verdict: the model is industry standard, the same Newson-Krumm model that OSRM, Meili, GraphHopper and barefoot use. The
implementation is necessarily ours. No permissive, embeddable, offline, cross-platform (Android and iOS) matcher exists that
would run on our scan-built graph. Embedding Valhalla or OSRM would add megabytes of native code plus a tile or preprocessing
pipeline, and the off-network state and fixed lag would still need tuning. **Keep ours.** Read Meili's
[implementation notes](https://valhalla.github.io/valhalla/meili/implementation_details/) and barefoot's online matcher for tuning
ideas (candidate clustering, breakage distance, interpolation).

### Geometry

| Crate | Licence | Latest | Downloads | Fit |
| --- | --- | --- | --- | --- |
| [`geo`](https://github.com/georust/geo) | MIT/Apache-2.0 | 0.33.1, 2026-04 | 23.7 M | `simplify` / `simplify_idx` (RDP) and `simplify_vw_preserve` (Visvalingam-Whyatt), plus Haversine, Geodesic and closest point. **No pinned-vertex option.** You get that by splitting at pins and simplifying each run, which is what our `simplify_pinned` already does. The crate is heavy (pulls `rstar`, `i_overlay`, `earcutr`, `robust` and others). |
| [`polyline`](https://github.com/georust/polyline) | MIT/Apache-2.0 | 0.11.0, **2024-05** | 462 k | `encode_coordinates(.., precision)` / `decode_polyline` with any precision (7 works). Depends on `geo-types`. Our version also rejects crafted overflow and off-globe input, so check `polyline` handles that before swapping. |
| [`rstar`](https://github.com/georust/rstar) | MIT/Apache-2.0 | 0.13.0, 2026-05 | 47.7 M | R*-tree, nearest-k / within-distance. Could replace our CSR grid cell index, but a uniform grid is just as good at our scale and more compact. |
| [`geo-index`](https://github.com/kylebarron/geo-index) | MIT/Apache-2.0 | 0.4.0, 2026-09 | 175 k | Static packed R-tree / KD-tree (flatbush port), zero-copy, compact. It is the best fit if we ever replace the grid. |

Verdict: these are the only places a library would clearly have saved work, about half a day to a day for DP plus polyline. The
code exists now, is about 100 lines and is tested. Swapping saves almost nothing and adds dependency weight (`geo`). **Keep ours.**
Use `geo` or `geo-index` only if new geometry work (polygon ops, buffers) shows up anyway. (Later the owner preferred
libraries over custom code: RDP moved to `geo`, the polyline codec stayed ours; see Outcome.)

### Graph / routing (Rust)

| Crate | Licence | Latest | Fit |
| --- | --- | --- | --- |
| [`petgraph`](https://github.com/petgraph/petgraph) | MIT/Apache-2.0 | 0.8.3, 2025-09 (545 M downloads) | Has CSR and Dijkstra / A\*. Its `Csr` type is close to ours, and a bounded multi-target Dijkstra (needed per HMM step) still needs custom code. It would have saved some Dijkstra boilerplate, maybe half a day. |
| [`fast_paths`](https://github.com/easbar/fast_paths) | MIT/Apache-2.0 | 1.0.0, 2024-05 | Contraction hierarchies. Overkill for short HMM hops (tens of metres), and preprocessing on the phone costs time. No. |
| [`osmpbf`](https://github.com/b-r-u/osmpbf) | MIT/Apache-2.0 | 0.3.8, 2025-10 | PBF reader. Not relevant: we read Overpass JSON. (`osmpbfreader` is WTFPL.) |

Verdict: bounded Dijkstra on CSR is the standard way to compute HMM transitions. `petgraph` would have saved a little but costs
our `u32` / `f32` memory layout. **Keep ours.**

### PDR, particle filter, step length

No maintained cross-platform PDR library exists. The available ones are Android research prototypes, e.g.
[Dead-Reckoning-Pro](https://github.com/SaturnXIII/Dead-Reckoning-Pro) and
[Zidane-Han/Pedestrian-Dead-Reckoning](https://github.com/Zidane-Han/Pedestrian-Dead-Reckoning). No Rust particle-filter crate is
mature (`kalman_filters` advertises one; the rest are SLAM or experimental). A bootstrap particle filter with systematic
resampling is about 150 lines, and we already depend on `rand`. Industry practice is to use OS step sensors (`TYPE_STEP_COUNTER`,
`CMPedometer`) and write step length and heading fusion yourself. **Build ours as planned.**

### Android / iOS layer

| Item | Fact | Recommendation |
| --- | --- | --- |
| FLP `QUALITY_HIGH_ACCURACY` | Gets 3D-mapping-aided GNSS and Google's fusion for free | Keep (already done). |
| MapLibre `LocationComponent` (custom-location mode) | MapLibre Native is BSD-2. It animates pushed locations and does not filter them. | Keep. Our estimates feed it. |
| MapLibre `LocationEngine` | Default engine wraps `LocationManager`; a Play services engine wraps FLP. No smoothing. | Nothing to gain. |
| **Fused Orientation Provider** | Play services 2024+. Fused accel/gyro/mag, quaternion, `headingDegrees`, **`headingErrorDegrees`**, true north. Foreground only. | **Adopt when GMS is present.** Its heading error can feed `held_flat_and_steady` and the carry-offset confidence directly. Fall back to `TYPE_ROTATION_VECTOR`. |
| Activity Recognition Transition API | Play services. Variable latency, noisy, confidence not calibrated. | Optional: use it as a soft prior on IMM mode probabilities (STILL or IN_VEHICLE). It is not needed for v1. |
| iOS | `CLLocationUpdate.liveUpdates` with `.fitness` / `.otherNavigation` (avoid automotive snapping), `CLHeading.headingAccuracy`, `CMPedometer`, `CMMotionActivityManager` | Same split as Android. The Rust core is shared, which is the main reason to keep everything in Rust. |

## D. Verdict

| Component | Industry standard? | Best existing library | Recommendation | Effort saved / switch cost |
| --- | --- | --- | --- | --- |
| OS fused fix, HIGH_ACCURACY | Yes (everyone) | FLP / Core Location | Keep | n/a |
| IMM Kalman (S/W/F CV, gating, relocation, hold) | Yes for IMM and gating (tracking literature, FilterPy). App-side filtering on top of FLP is common practice; big apps do not publish theirs. | None with IMM (`adskalman` / `kfilter` are plain KF; `filter` is dead) | Keep | A crate would have saved under 1 day. Switching now costs about 1 day plus re-tuning. |
| Hand-rolled 2x2 / 4x4 matrices | Common in embedded filters | `nalgebra` | Keep | Switching adds compile time and binary size and gains nothing. |
| Douglas-Peucker with pins | Yes (RDP) | `geo::Simplify` (no pins; split runs yourself) | Keep the pin split. (RDP itself later moved to `geo`, see Outcome.) | Would have saved about half a day. Switch costs a heavy dependency. |
| Encoded polyline 1e7 | Yes (Google format, precision 6 or 7 common) | `polyline` 0.11 (MIT/Apache) | Keep (our hardened decode is tested) | Would have saved about 2 hours. Switching saves 50 lines and adds `geo-types`. |
| Street graph (CSR + grid + Dijkstra) | Yes (CSR + bounded Dijkstra is how matchers do HMM transitions) | `petgraph` Csr/Dijkstra; `geo-index` / `rstar` for the index | Keep | Would have saved about half a day. Switching loses the compact `u32` / `f32` layout. |
| HMM map matcher (Newson-Krumm, off-network, fixed lag) | **Yes**: OSRM, Meili, GraphHopper, barefoot, FMM all use it. Live on-device pedestrian matching is rare. | None embeddable and permissive in Rust (`routers` is GPL; Meili/OSRM are heavy C++ servers) | Keep. Borrow tuning ideas from Meili and barefoot. | Effectively nothing to save. Embedding Valhalla would cost weeks plus MBs. |
| Compass heading / held-flat rule | Fused orientation is standard (FOP is Google's productised version; Google Maps' heading cone is likely built on the same fusion, though that is not confirmed) | **Google FOP** (GMS), `CLHeading` on iOS | **Adopt FOP on GMS phones**, keep the rotation-vector fallback | About 0.5 to 1 day to add. Gives a real heading error instead of a spread heuristic. |
| Carry offset (pocket heading vs course) | Research topic (PDR "misalignment estimation"). No platform API. | None | Keep | n/a |
| PDR particle filter bridge + step length | PDR is standard in indoor research. No productised SDK. | None mature (Rust or mobile) | Build ours as planned, on OS step sensors | n/a |
| Motion mode for IMM | Activity recognition is a common soft prior | Activity Recognition API / CMMotionActivity | Optional later: soft prior only | About 1 day. Small gain, since the IMM already infers the mode from the data. |
| Pin rendering | Yes | MapLibre LocationComponent (BSD-2) | Keep | n/a |

## Top recommendations

1. Keep the IMM, the HMM matcher and the graph. They follow the textbook (Newson-Krumm) design, and no permissive, offline Rust
   library covering Android and iOS exists to replace them.
2. Adopt Google's Fused Orientation Provider for compass heading on Play services phones. Feed `headingErrorDegrees` into the
   held-flat rule and the carry-offset confidence, and keep `TYPE_ROTATION_VECTOR` for phones without Play services.
3. Do not swap Douglas-Peucker or the polyline code for `geo` / `polyline` now. Saving 100 tested lines is not worth `geo`'s
   dependency weight. Revisit only if other `geo` features become necessary.
4. Tune the matcher against Valhalla Meili's implementation notes and barefoot's online matcher (candidate radius, breakage
   distance, the off-network transition) instead of inventing new heuristics.
5. On iOS, use `CLLocationUpdate` with `.fitness` or `.otherNavigation`, never `.automotiveNavigation`, so Core Location does not
   road-snap before our filter. Optionally add activity recognition later as a soft IMM prior.
