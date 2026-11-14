# Development roadmap

This roadmap distinguishes code that is already present from work that still needs implementation or platform verification.

## Phase 1 — Foundation

- [x] Create the Flutter application and dark dashboard.
- [x] Create the Rust core crate.
- [x] Add HTTP(S) URL validation.
- [x] Add the Flutter–Rust bridge and generated Dart bindings.
- [x] Add GitHub Actions workflows for project checks.
- [x] Obtain clean CI runs on GitHub-hosted runners.

## Phase 2 — Direct downloads

- [x] Implement streamed HTTP(S) downloads in bounded buffers in Rust.
- [ ] Implement parallel HTTP Range segmented downloads with bounded concurrency, per-segment recovery, and safe final assembly (see `docs/SEGMENTED_DOWNLOADS.md`).
- [x] Stream progress events through the Flutter–Rust bridge.
- [x] Add active-task pause, resume, and cancel controls.
- [x] Add a Flutter download list with search, sorting, and file-type filters.
- [x] Persist download-list history locally.
- [x] Add retry, queue actions, and basic history management in the UI.
- [ ] Verify transfer behavior with real files and network edge cases.
- [x] Add HTTP range-based resume with validator-aware recovery.
- [x] Add temporary-file handling, safe finalization, and protected resume metadata.
- [x] Add user-selected save locations and destination conflict controls.
- [ ] Add durable task-state persistence and recovery.
- [ ] Add macOS desktop support (`macos-x86_64` and `macos-aarch64`) across Flutter, Rust target detection, packaging, and CI; do not claim support until release artifacts and tests pass.

## Phase 3 — Video and audio

### yt-dlp: standalone tool manager

- [x] Add the initial Rust yt-dlp adapter for version probing and single-source HTTP(S) metadata inspection.
- [x] Stage official desktop release metadata and SHA-256 digests for supported targets.
- [ ] Implement an independent yt-dlp manager screen/settings section in Flutter.
- [x] Read the latest stable release from the official yt-dlp GitHub Releases API; do not use third-party mirrors.
- [ ] Compare installed and latest versions and show release details before updating.
- [ ] Add automatic update checks on app startup and a configurable periodic check while the app is running.
- [ ] Add a manual "Check for updates" action and a separate "Update now" action.
- [ ] Download the correct official asset for the exact OS/CPU target with bounded streaming and clear progress.
- [ ] Verify the downloaded asset against the SHA-256 digest published by the official release metadata before installation.
- [ ] Install into an app-managed, versioned tools directory using staging and atomic activation; never overwrite a working executable in place.
- [ ] Keep the previous working version available for rollback if activation or the post-install version probe fails.
- [ ] Persist the installed version, last successful check, update channel/status, and a bounded update history.
- [ ] Make network failures, rate limits, unsupported targets, checksum mismatches, and disk errors visible without damaging the installed version.
- [ ] Keep yt-dlp updates independent from application releases and from FFmpeg updates.
- [ ] Resolve the installed executable through the managed-tools layer and connect it to the Flutter video workflow.
- [ ] Validate the updater on Windows and Linux; treat Android as unsupported until a compatible official/runtime distribution is verified.

### Video and audio workflow

- [x] Add a Rust format catalogue for yt-dlp metadata, excluding DRM-only entries and signed media URLs.
- [x] Add video URL inspection and quality/format selection UI.
- [x] Connect selected formats to the resumable queue with per-request headers.
- [x] Add the initial safe Rust FFmpeg adapter for version probing and local video-to-MP3 conversion, with bitrate selection and progress events.
- [ ] Add pause/cancel controls and robust temporary-output cleanup to FFmpeg conversion.
- [ ] Connect MP3 conversion to the Flutter workflow and download/task UI.
- [ ] Build a separate FFmpeg settings/status section and independent update/install lifecycle.
- [x] Add FFmpeg latest-release discovery for Windows/Linux desktop targets using BtbN release metadata and its published checksums.sha256 file.
- [ ] Download the selected static GPL archive with bounded streaming, verify its SHA-256, safely extract only ffmpeg, and probe the staged executable.
- [ ] Activate FFmpeg by versioned atomic pointer with a verified previous-version rollback path.
- [ ] Verify and document trusted FFmpeg binary providers per platform; FFmpeg upstream publishes source code and links to independent binary maintainers.
- [x] Assess the upstream Android FFmpeg AAR runtime blockers and document a separate NDK-built, ABI-verified Android runtime plan.
- [x] Set Android FFmpeg feature minimum to API 34 while preserving the app-wide API 24 minimum; target NDK r28+ and 16 KB page-size compatibility.
- [ ] Audit every native library and APK ZIP alignment before introducing the Android FFmpeg runtime.
- [ ] Build and publish a reproducible Android FFmpeg executable with the required MP3 encoder and 16 KB-compatible ELF dependencies.
- [ ] Verify archive checksums/signatures, extraction, executable permissions, versioned activation, and rollback behavior.
- [ ] Complete real-site and Windows/Linux runtime verification.

## Phase 4 — Advanced features

- [ ] Add BitTorrent support.
- [ ] Add browser link handoff and browser integration.
- [x] Receive shared HTTP(S) URLs from other apps through Android Share sheet, including cold-start and already-running app delivery.
- [x] Inspect shared video URLs on Android with the bundled yt-dlp runtime and present source-derived combined video/audio quality options.
- [x] Route selected combined video formats on Android through the native yt-dlp runtime into app-private storage before the existing Android save-folder handoff.
- [ ] Add source-derived audio-only quality options and verified MP3 conversion on Android after the independent FFmpeg runtime is integrated.
- [ ] Design and implement the floating video-capture indicator.
- [ ] Add polished platform-specific UX and accessibility improvements.

## Definition of done

A feature is complete only when its implementation is present, relevant automated checks pass, and target-platform behavior has been verified. A UI placeholder, manifest entry, or untested code path does not count as a completed feature.

## Cross-platform release verification

- [ ] Verify Android storage access and background execution on real devices.
- [ ] Verify Windows save locations, cancellation, and packaging.
- [ ] Verify Linux save locations, cancellation, and packaging.
- [ ] Add platform-specific build and release workflows.
- [ ] Test large files, interrupted connections, low storage, and application shutdown.
