# Architecture (initial)

Easy Download is a Flutter application with a Rust core.

- `lib/`: Flutter presentation layer and download-task domain model.
- `rust/`: Rust core with HTTP(S) URL validation and a blocking direct-download function.
- The Rust downloader streams response data to a destination file in chunks and can report received bytes and optional total bytes through a callback.
- `rust/src/api.rs` now defines a small bridge-facing API for URL validation and direct downloads. A generated Flutter-to-Rust bridge has **not** been added yet, so the Flutter interface still cannot start this Rust downloader.
- Pause/resume, cancellation, retries, persistent queue storage, video extraction, BitTorrent, browser integrations, and platform packaging are **not implemented yet**.

## Planned boundaries

1. Flutter owns screens, user interaction, and platform-specific presentation.
2. Rust owns download jobs, network transfers, and progress reporting.
3. `flutter_rust_bridge` v2 is the selected typed bridge. Its generated bindings still need to be integrated and verified; the initial Rust API currently exposes URL validation and a basic direct-download call.
4. Platform capabilities and storage permissions must be handled explicitly per target OS.
5. Downloaded files should be written to a temporary destination and finalized safely when the download completes; this behavior is not implemented yet.

Keep this document updated as implementation decisions are tested. Code written to the repository is not considered verified until it builds and its tests run successfully.
