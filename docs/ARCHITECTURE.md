# Architecture (initial)

Easy Download is a Flutter application with a Rust core.

- `lib/`: Flutter presentation layer and download-task domain model.
- `rust/`: Rust core with HTTP(S) URL validation and a blocking direct-download function.
- The Rust downloader streams response data to a destination file in chunks and can report received bytes and optional total bytes through a callback.
- A Flutter-to-Rust bridge has **not** been added yet, so the Flutter interface cannot start this Rust downloader.
- Pause/resume, cancellation, retries, persistent queue storage, video extraction, BitTorrent, browser integrations, and platform packaging are **not implemented yet**.

## Planned boundaries

1. Flutter owns screens, user interaction, and platform-specific presentation.
2. Rust owns download jobs, network transfers, and progress reporting.
3. A typed bridge will communicate job creation, pause/resume/cancel, and progress events between Flutter and Rust.
4. Platform capabilities and storage permissions must be handled explicitly per target OS.
5. Downloaded files should be written to a temporary destination and finalized safely when the download completes; this behavior is not implemented yet.

Keep this document updated as implementation decisions are tested. Code written to the repository is not considered verified until it builds and its tests run successfully.
