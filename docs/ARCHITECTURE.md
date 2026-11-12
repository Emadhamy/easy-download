# Architecture

Easy Download uses Flutter for presentation and Rust for direct download operations.

## Current structure

- `lib/main.dart` — Flutter dashboard, link list, local history, download controls, filtering, and progress presentation.
- `lib/src/rust/` — generated Dart bindings produced by `flutter_rust_bridge_codegen`.
- `rust/src/lib.rs` — HTTP(S) URL validation and chunked file-transfer implementation.
- `rust/src/api.rs` — bridge-facing functions for starting controlled downloads, receiving progress events, and pausing, resuming, or cancelling active tasks.
- `rust/src/managed_tools.rs` — typed parsing and validation of yt-dlp/FFmpeg release metadata.
- `rust/src/yt_dlp.rs` — a process adapter for version probing and single-source metadata inspection. It requires an executable path supplied by the managed-tools layer; it does not install the executable or download media.
- `rust/resources/managed-tools/` — manifest and JSON Schema for future independently managed external tools.

## Current download flow

1. Flutter asks the Rust API to validate a direct HTTP(S) URL.
2. The user adds the validated link to the local download list.
3. When started, Flutter selects a destination in the app documents directory and creates a task ID.
4. The generated bridge calls the Rust controlled-download API.
5. Rust streams the response to the destination and sends progress events to Flutter.
6. Flutter updates received bytes, total size, and estimated transfer speed. Pause, resume, and cancel commands are sent to Rust using the task ID.
7. The download list is persisted locally. Active network tasks do not survive an application restart; restored entries return to an inactive/queued state.

## Important boundaries and gaps

- The current direct-download implementation is not a resumable-download engine. Pause/resume controls affect an active transfer; they do not provide HTTP range-based continuation after an app restart or process loss.
- The current destination is the app documents directory. User-selected folders, Android storage permissions, background execution, and platform-specific file access still need dedicated implementation and device testing.
- The current UI classifies file names by extension. This is not video extraction or media conversion.
- The yt-dlp adapter can inspect one HTTP(S) source and return JSON metadata, but it is not yet connected to a verified installed executable or the Flutter UI. It does not download media or select formats.
- yt-dlp and FFmpeg release metadata remains disabled and empty in the checked-in manifest. No executable is downloaded or enabled. Android still needs a separately validated compatible yt-dlp runtime.
- BitTorrent, browser integration, floating video capture, and production packaging are not implemented.
- The current app settings action is a placeholder notice, not a complete settings screen.

## Engineering rules

1. Flutter owns screens, user interaction, local presentation state, and platform-facing UI.
2. Rust owns network transfer behavior and download control primitives.
3. Keep generated bridge files synchronized with the Rust API by running `flutter_rust_bridge_codegen generate`.
4. Do not enable an external binary until its official release URL, target mapping, archive layout, and independently verified SHA-256 digest are known.
5. Never describe a feature as verified until its build and relevant tests have actually passed on the target platform.
