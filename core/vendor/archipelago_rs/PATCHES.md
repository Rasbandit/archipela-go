# Local patches to archipelago_rs 3.0.1 (MIT, nex3/archipelago_rs)

- `src/cache.rs`: add a fallback `platform_cache_dir()` (returns `None`) for targets other than Windows/macOS/Linux so the crate compiles for Android. Candidate for an upstream PR.

- `src/connection/socket.rs`, `src/connection.rs`, `src/client.rs`: add `Readiness` (`Connection::readiness`, `Client::readiness`,
  `Readiness::new`), a clone of the socket's `Arc<Async<TcpStream>>` whose `wait(timeout)` blocks until the socket is readable. The
  crate only offers non-blocking `update()`, so a host had to poll on a timer; this lets it sleep until the server sends something
  (battery) without holding the connection. Tested in `apgo-ffi` (`ap_wait_tests`). Candidate for an upstream PR.

- `src/connection/socket.rs`, `src/error.rs`, `src/connection.rs`, `Cargo.toml` (2026-10-12, #166): the WebSocket layer is
  **soketto 0.8 with permessage-deflate** instead of tungstenite. Archipelago's protocol says uncompressed connections are deprecated
  and may stop working, and the 0.6.8 server warns every client without compression; tungstenite has had no permessage-deflate
  since the request in 2017 (issue snapview/tungstenite-rs#2; PRs #426 and #561 unmerged), so carrying a fork was rejected.
  - Same outward API: `Socket::{connect, recv_all, try_recv, recv_async, send, readiness}`, `Readiness`, the wss-then-ws fallback.
  - soketto is async: the next receive is one persistent future (never dropped mid-frame), polled with a no-op waker by
    `read_inner` until it has to wait, which answers pings on the way, so the `Readiness` socket wait still works. `send` blocks
    until the frame is out while also driving that future (it may hold the writer while answering a ping).
  - TLS is `futures-rustls` (rustls only; the `native-tls` feature is gone). `Error::WebSocket` wraps the crate's own `WsError`.
  - flate2's `zlib` backend links Android's system `libz.so` (an NDK stable API).
  - Tested in `apgo-ffi` (`ap_handshake_tests`: bad URLs, the upgrade request offers compression) and against the pinned server
    (`the_connection_is_compressed_so_the_server_never_warns`, `just test-ap`).

## Evidence (2026-10-07)
Minimal repro with the unpatched crate, no TLS features, no other code:
`archipelago_rs = { version = "=3.0.1", default-features = false }` then `cargo check` passes on host Linux and fails with
`error[E0308]: mismatched types` at `src/cache.rs:31` on `aarch64-linux-android` (cargo-ndk) and `x86_64-unknown-freebsd`.
Upstream `main` (33ddae6, 2026-06-25) has identical `cache.rs`; the repo has no issue mentioning Android.
Not yet known: whether the maintainer wants non-desktop targets supported. Do not file or PR until the app is proven; ask via an issue first.
