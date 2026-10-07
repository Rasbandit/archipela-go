# Context Doc: Archipelago Client Libraries

_Last verified: 2026-10-07_

## Status
Working survey. Facts from GitHub API/READMEs read 2026-10-07; items marked (unverified) came from web search only.

## What This Is
Existing client libraries implementing the Archipelago WebSocket protocol, and a recommendation for a Rust-core + Kotlin Android app.

## Environment
Target: Android first (iOS later), Rust core shared, Kotlin UI. Protocol target: 0.6.x (latest release 0.6.8, 2026-10-04; 0.7.0 unreleased).
Official list: "network protocol.md" library table (Python CommonClient, SNIClient, JVM, .NET, C++, JS/TS, Haxe, Rust, Lua, GameMaker).

## Connection
N/A. All libs wrap `ws(s)://host:port` JSON protocol (see `archipelago-network-protocol.md`).

## Auth
N/A.

## Key Commands / Patterns

| Lang | Lib | License | Latest / activity | Notes |
|---|---|---|---|---|
| Rust | `archipelago_rs` (repo now github.com/nex3/archipelago_rs; docs link ryanisaacg/archipelago_rs redirects/old) | MIT | Cargo version 3.0.1; last commit 2026-06-25 "Report network protocol version 0.6.6"; no GitHub releases | Non-blocking poll design (`Connection.update`/`Client.update`), no async runtime needed (uses smol, tungstenite, serde). Tracks server state (hint points, checked locations), `oneshot` receivers for Scout/Get. TLS via rustls with native-tls fallback (default features both). Interned `Ustr` strings (memory never freed). DeathLink supported. Reports version 0.6.6 (not 0.6.8; server accepts as compatible per major.minor - unverified). Edition 2024. |
| JVM/Kotlin | `io.github.archipelagomw:Java-Client` (github.com/ArchipelagoMW/Java-Client, aka Archipelago.MultiClient.Java) | MIT | 0.2.1 (2025-09-26) on Maven Central; last commit 2026-08-13 (readme only) | Java 8 target, deps: Gson (api), Java-WebSocket, Apache httpclient/httpcore. Pre-1.0; package renamed from `dev.koifysh.archipelago` in 0.2.0. Not KMP. Android compatibility of Java-WebSocket/httpclient5 not verified here; httpclient5 is heavy on Android (unverified). Protocol version supported not stated in README; check `Client.java`. |
| TS/JS | `archipelago.js` (npm; github.com/ThePhar/archipelago.js) | MIT | 2.1.0 (2026-04-11) adds `CreateHints`/`UpdateHint` | Zero deps, runtime-agnostic (browser + Node >=18), typed, docs archipelago.js.org. Best-maintained 0.6 client; irrelevant for native Android unless using WebView/React Native. |
| C# | `Archipelago.MultiClient.Net` (NuGet 6.7.1, 2026-03-21 per search) | MIT | last commits 2026-03-21 (individual datapackage responses, UpdateConnectionOptions fix); added CreateHints/UpdateHint 2025-10 | Official-org repo; "conforms to latest stable protocol". Most used in game mods (Unity/BepInEx). Docs: archipelagomw.github.io/Archipelago.MultiClient.Net. |
| Python | `CommonClient.py` in main repo (reference); `archipelagopy` on PyPI 0.1.3 (unverified, small) | main repo license unverified (believed MIT); archipelagopy unverified | CommonClient tracks core | Reference behavior; use as ground truth when docs are unclear. |
| C++ | `apclientpp` (black-sliver, header-only), `APCpp` | n/a read | n/a | Listed in official docs; not evaluated. |
| Other | hxArchipelago (Haxe, updated 2026-07), lua-apclientpp, gm-apclientpp | n/a | n/a | Listed only. |

Kotlin/KMP: no established Kotlin Multiplatform or Android-native Archipelago library found (GitHub search "archipelago multiworld client kotlin OR android" returned only game-specific clients; Perplexity also found none). No Android app for Archipelago found in this research (not exhaustive).

### Recommendation (Rust core + Kotlin Android)
1. **Use `archipelago_rs` inside the Rust core**, exposed to Kotlin via UniFFI (or JNI) with a thin event/command interface. It is the only maintained, protocol-complete (0.6.x, hints, DataStorage, scouts, DeathLink), MIT library in Rust, designed to be polled from a loop (fits a core with its own tick) and avoids Android Java-dependency questions. iOS later reuses the same core.
2. **Spike first** (small step): build `archipelago_rs` for `aarch64-linux-android` with rustls only (disable `native-tls` default feature; it needs OpenSSL/system TLS) and connect to archipelago.gg. Verify DataPackage, `ReceivedItems` index handling, reconnect.
3. **Fallback** if the spike fails: `Java-Client` directly from Kotlin (works but pre-1.0, bundles old-style deps; drops Rust sharing) or hand-roll the protocol in Rust on tokio-tungstenite/serde (protocol is small; this doc + protocol doc suffice).
4. Do not choose archipelago.js/MultiClient.Net unless the architecture changes (WebView/RN, or .NET/MAUI).
5. Wrap the library behind our own trait (`ArchipelagoTransport`/events) so it can be swapped, and so we can pin a 0.6.x version string.

## Failed Approaches / Dead Ends
- None built yet. `ArchipelagoMW/archipelago.js` does not exist (404): the repo is `ThePhar/archipelago.js`. Original `ryanisaacg/archipelago_rs` URL in protocol doc is stale for latest activity; use `nex3/archipelago_rs`.

## Gotchas
- `archipelago_rs` default features pull both rustls and native-tls; on Android disable native-tls. TLS failure falls through to unencrypted ws (library tries rustls, native-tls, then plain TCP) - for a mobile app consider forcing wss-only for archipelago.gg to avoid silent downgrade (security).
- `archipelago_rs` interns strings forever (`Ustr`); acceptable for one room per session, but long-lived multi-room use grows memory.
- It reports protocol 0.6.6 while server is 0.6.8; re-check on the 0.7.0 release (changes unknown).
- Rust crate is at 3.0.1 but has no GitHub releases/tags verified; confirm crates.io listing before depending on it (not verified; search could not confirm).
- Java-Client is 0.x: expect API churn; official-org but single-maintainer-feeling activity (verify before relying).
- Libraries do not implement an Android foreground service / background connection policy; that is our app's job (reconnect + persisted last item index).

## References
- https://github.com/ArchipelagoMW/Archipelago/blob/main/docs/network%20protocol.md (library table)
- https://github.com/nex3/archipelago_rs (README, Cargo.toml read)
- https://github.com/ArchipelagoMW/Java-Client (readme, build.gradle.kts read)
- https://github.com/ThePhar/archipelago.js (package.json, releases read)
- https://github.com/ArchipelagoMW/Archipelago.MultiClient.Net
- Related: `archipelago-network-protocol.md`, `archipelago-concepts.md`
