# Context Doc: Quest Types and Phone APIs (design brainstorm)

_Last verified: 2026-10-07 (API facts from Perplexity summaries of developer.android.com; items marked (verify) are unconfirmed)_

## Status
Ideas, not commitments. v1 ships only `reach_point`. Goal from the owner: "a genuinely fun game out of walking", using as many phone capabilities as sensible, all opt-in.

## Design Principles
1. **Opt-in per capability.** Every quest type declares `needs: [capabilities]`. The player grants OS permissions in the app; nothing is requested up front.
2. **Generation precedes permissions.** The apworld cannot know what the phone will allow. Add a YAML option (e.g. `quest_types`, an OptionSet) so only chosen types are generated, and give each slot a `fallback` quest of equal difficulty (e.g. heart-rate zone -> timed brisk walk) if the capability is missing at play time.
3. **Privacy by design.** Sensor and health data stay on the phone. Only the check ("slot 17 done") leaves it.
4. **Verification tiers.** `sensor-verified` (cross-checked, e.g. steps vs GPS distance), `device-verified` (OS says so), `honor` (button). Cooperative game: keep it light, but cheap cross-checks help.
5. **Safety and access.** Never route onto roads/private land/at unsafe hours; "night" quests opt-in; every quest has a non-step variant (wheelchair/roll, bike, transit modes). Show a safety note.
6. **Pace mix.** Blend 5-minute quests, hour-long quests, passive background quests (steps), and a few "boss" quests as goal gates.

## Phone API Map (Pixel 8 Pro, Android 17)
| API | Gives | Permission (user grants) | Notes |
|--|--|--|--|
| Fused/GPS location | position, speed, accuracy, altitude | `ACCESS_FINE_LOCATION`, `ACCESS_BACKGROUND_LOCATION`, foreground service type `location` | have it; needs a foreground service for background |
| Geofencing (Play services) | enter / exit / DWELL events with app closed | fine + background location | dwell = "stay 5 minutes"; per-app fence limit (verify, ~100) |
| Step counter / detector sensor | cumulative steps since boot | `ACTIVITY_RECOGNITION` | subtract baselines; resets on reboot |
| Health Connect | steps, distance, heart rate, exercise sessions, floors, speed, sleep | per type `READ_STEPS`, `READ_HEART_RATE`, `READ_EXERCISE` (+ `READ_HEALTH_DATA_IN_BACKGROUND`) via the Health Connect UI | `metadata.recordingMethod` flags `MANUAL_ENTRY` (filter cheating); heart rate only exists if a watch/app wrote it (phone has no HR sensor); ~30-day history by default |
| Activity Recognition (Play services) | walking, running, cycling, in-vehicle, still | `ACTIVITY_RECOGNITION` | great for mode checks (walk vs drive) |
| Barometer (`TYPE_PRESSURE`) | relative altitude, floors, elevation gain | none | far better than GPS altitude (verify Pixel 8 Pro has one) |
| Magnetometer / rotation vector | compass heading | none | "face north", orienteering |
| Accelerometer / gyroscope | reps, shakes, stillness, cadence | none (high sample rates need a special permission) | jumping jacks, push-ups, "stand still" |
| Ambient light | outdoors vs indoors, dusk | none | corroborates "touch grass" |
| Camera + ML Kit on-device labeling | photo quests ("a dog", "a bridge") | `CAMERA` | no upload; label on device |
| Bluetooth LE / Nearby | HR straps, car-connection = driving, nearby friends | `BLUETOOTH_CONNECT`, `BLUETOOTH_SCAN` | co-op proximity |
| NFC | tap physical tags/stamps | `NFC` | community "stamp posts" |
| Time/sun | sunrise, sunset, golden hour computed locally | none | no network |
| Weather (Open-Meteo, free, network) | rain, temperature, wind | `INTERNET` | rain/cold bonus quests |
| Notifications | quest alerts | `POST_NOTIFICATIONS` | respect quiet hours |
| Wear OS / watch via Health Connect | HR zones, workouts | Health Connect grants | optional |

## Quest Ideas
Columns: how it plays | verified by | needs | YAML knobs. Effort: S small, M medium, L large.
### Place and movement
| Quest | How it plays | Verified by | Needs | Knobs | Effort |
|--|--|--|--|--|--|
| Reach point | go to a place (v1) | geofence | location | distance tier, mode | S (done) |
| Dwell | go to A and stay 5 minutes | geofence dwell | location | minutes, radius | S |
| Courier | go to A, then to B within N minutes | two geofences + timer | location | time, distance | M |
| Fetch and return | go to A, come back home with the "item" | geofence A then home | location | distance, time | S |
| Round trip | get >= X m from home and return within Y min | GPS track | location | X, Y | S |
| Loop | return to start without retracing a street | GPS track, polyline overlap | location | length | M |
| Waypoint chain | A, then B, then C in order | ordered geofences | location | count | M |
| Explorer | visit N distinct map cells ("fill the map") | cell set from GPS | location | N, cell size | M |
| Neighborhood sweep | cover X% of streets in a drawn zone | street coverage | location | percent | L |
### Time
| Quest | How it plays | Verified by | Needs | Knobs | Effort |
|--|--|--|--|--|--|
| Away from home | be >= 1 km from home for N hours total today | geofence exit + timer | background location | hours | S |
| Adventurer's day | visit 3 different places in one day | geofences | location | count | S |
| Sunrise / golden hour | be at a spot within 30 min of sunrise/sunset | geofence + local sun time | location | offset | S |
| Night owl (opt-in) | short well-lit walk after dark | time + steps | location, safety opt-in | length | S |
### Steps and fitness
| Quest | How it plays | Verified by | Needs | Knobs | Effort |
|--|--|--|--|--|--|
| Step milestones | every 2,500 lifetime steps = a check (passive) | step counter | activity recognition | stride | S |
| Daily goal | 8,000 steps today | step counter / Health Connect | activity or HC | goal | S |
| Streak | goal 3 days in a row | daily totals | same | days | M |
| Pace | hold 6 km/h for 10 min (jog) | GPS speed + steps | location | speed, minutes | M |
| Heart zone | keep HR in zone 2 for 15 min | Health Connect / BLE strap | HR source | zone, minutes | M |
| Climb | gain 50 m elevation | barometer + GPS | none or location | meters | S |
| Summit | reach a high point | geofence + elevation gain | location | height | M |
| Rep quests | 20 jumping jacks / push-ups | accelerometer rep counter | none | reps | M |
### Fetch, camera, fun
| Quest | How it plays | Verified by | Needs | Knobs | Effort |
|--|--|--|--|--|--|
| Photo fetch | photograph "a dog / tree / mural / bridge" | on-device ML labeling | camera | category | M |
| Stamp hunt | tap an NFC/QR stamp post | tag read | NFC or camera | locations | M |
| Letter hunt | find a street whose name starts with a letter | zone street names | location | letters | M |
| Touch grass | stand outdoors, still, 3 min | light + accelerometer + GPS | none | minutes | S |
| Digital detox | walk 10 min with screen off | screen state + steps | activity | minutes | S |
| Compass run | walk 500 m heading north | magnetometer + GPS | location | heading, length | S |
### Social and team (Archipelago features)
| Quest | How it plays | Verified by | Needs | Knobs | Effort |
|--|--|--|--|--|--|
| Raid boss | the whole team reaches N total steps this week | shared counter in server DataStorage (verify) | activity | total | M |
| Rescue relay | your steps unlock a teammate's check (Bounce/DataStorage) | server messages | activity | steps | L |
| Meet up | two players within 50 m of each other | BLE/Nearby | bluetooth | radius | L |
### Weather and traps
| Quest | How it plays | Verified by | Needs | Knobs | Effort |
|--|--|--|--|--|--|
| Rain walk | do any walk while it rains | weather API + steps | network | minutes | S |
| Cold snap | walk when temp < X | weather API | network | degrees | S |
| Traps (items) | Fog of War hides the map, Shuffle rerolls places, Silence mutes, honor pushes | app state | none | rate | S |

## Suggested Order
1. Dwell + Courier + Fetch/return (geofence only, reuses what works).
2. Step milestones + Away from home (passive, big feel, background service).
3. Elevation/Climb (barometer) and Explorer (cells).
4. Health Connect (steps/HR) as an optional power-up tier.
5. Camera quests, then team features once the core is stable.

## Open Questions
- Contract shape: `type`, `params`, `needs`, `fallback` per slot (bump `schema_version`).
- Which quests may be "required" for logic (keys behind a hard quest) vs optional filler.
- Banning/rerolling a quest type mid-game without changing the check.
- Battery cost of continuous sensing; foreground service rules (verify FGS types on Android 17).

## References
`docs/context/archipelago-game-model.md`, `apworld/docs/contract.md`, `docs/context/android-dev-workflow.md`
