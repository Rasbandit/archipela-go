# Local patches to archipelago_rs 3.0.1 (MIT, nex3/archipelago_rs)

- `src/cache.rs`: add a fallback `platform_cache_dir()` (returns `None`) for targets other than Windows/macOS/Linux so the crate compiles for Android. Candidate for an upstream PR.

## Evidence (2026-10-07)
Minimal repro with the unpatched crate, no TLS features, no other code:
`archipelago_rs = { version = "=3.0.1", default-features = false }` then `cargo check` passes on host Linux and fails with
`error[E0308]: mismatched types` at `src/cache.rs:31` on `aarch64-linux-android` (cargo-ndk) and `x86_64-unknown-freebsd`.
Upstream `main` (33ddae6, 2026-06-25) has identical `cache.rs`; the repo has no issue mentioning Android.
Not yet known: whether the maintainer wants non-desktop targets supported. Do not file or PR until the app is proven; ask via an issue first.
