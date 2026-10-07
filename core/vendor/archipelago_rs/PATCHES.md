# Local patches to archipelago_rs 3.0.1 (MIT, nex3/archipelago_rs)

- `src/cache.rs`: add a fallback `platform_cache_dir()` (returns `None`) for targets other than Windows/macOS/Linux so the crate compiles for Android. Candidate for an upstream PR.
