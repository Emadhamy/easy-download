# Architecture

Easy Download uses Flutter for presentation and Rust for download operations and managed external tools.

## Current structure

- `lib/main.dart` — Flutter dashboard, download list, local history, download controls, filtering, and progress presentation.
- `lib/src/rust/` — generated Dart bindings produced by `flutter_rust_bridge_codegen`.
- `rust/src/lib.rs` — HTTP(S) validation and file-transfer implementation.
- `rust/src/api.rs` — bridge-facing functions for downloads, download controls, platform detection, managed-tool release metadata, and yt-dlp inspection.
- `rust/src/managed_tools.rs` — typed parsing and validation of yt-dlp/FFmpeg release metadata and platform targets.
- `rust/src/yt_dlp.rs` — process adapter for version probing and single-source metadata inspection. It requires an executable path supplied by the managed-tools layer.
- `rust/resources/managed-tools/` — release manifest and JSON Schema for independently managed external tools.

## Managed tool update design

yt-dlp and FFmpeg are separate managed products inside Easy Download, not application-bundled files that only change when the app is upgraded.

For yt-dlp, the intended lifecycle is:

1. Query the official yt-dlp GitHub Releases API for the latest stable release.
2. Resolve an asset only for an explicitly supported OS/CPU target; never infer Android compatibility from a Linux binary.
3. Compare the remote version with the installed version and show the result in the dedicated yt-dlp manager.
4. Check for updates at app startup and on a configurable interval while the app is running; also provide a manual check.
5. Download the official asset to a staging location with bounded streaming and visible progress.
6. Verify its SHA-256 against the digest published by the official release metadata. Reject missing or mismatched digests.
7. Activate the verified executable from a versioned app-managed directory without overwriting the currently working version in place.
8. Probe the newly installed executable. If activation or the version probe fails, keep or restore the previous working version.
9. Record installed version, last check, result, and a bounded update history.

The updater must fail closed on API errors, rate limits, unsupported targets, missing assets/digests, checksum mismatch, invalid executables, and disk errors. A failed update must not break an already installed working version. yt-dlp updates must not trigger an app update and must not modify FFmpeg. FFmpeg will have its own independent manager and lifecycle.

## Current implementation boundary

The repository currently contains the typed release-manifest foundation, staged official desktop yt-dlp metadata, platform target resolution, and a yt-dlp process adapter. It now contains an official GitHub latest-stable-release lookup that selects a supported desktop asset and requires GitHub's published SHA-256 digest. It does not yet contain the complete asset download, checksum verification of downloaded bytes, safe installation, rollback, persisted update history, periodic scheduler, or dedicated Flutter manager UI. The manifest deliberately keeps tool installation disabled until that lifecycle is implemented and tested.

## Engineering rules

1. Flutter owns screens, user interaction, local presentation state, and platform-facing UI.
2. Rust owns network transfer behavior, trusted release resolution, binary verification, and safe managed-tool installation.
3. Keep generated bridge files synchronized with the Rust API by running `flutter_rust_bridge_codegen generate`.
4. Never enable an external binary until its official source, target mapping, digest verification, installation path, and rollback behavior are implemented.
5. Never describe a feature as verified until its build and relevant tests have actually passed on the target platform.
