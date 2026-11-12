# Development roadmap

This roadmap distinguishes code that is already present from work that still needs implementation or platform verification.

## Phase 1 — Foundation

- [x] Create the Flutter application and dark dashboard.
- [x] Create the Rust core crate.
- [x] Add HTTP(S) URL validation.
- [x] Add the Flutter–Rust bridge and generated Dart bindings.
- [x] Add GitHub Actions workflows for project checks.
- [ ] Obtain a clean, current CI run after the runner-startup issue is resolved.

## Phase 2 — Direct downloads

- [x] Implement chunked HTTP(S) downloads in Rust.
- [x] Stream progress events through the Flutter–Rust bridge.
- [x] Add active-task pause, resume, and cancel controls.
- [x] Add a Flutter download list with search, sorting, and file-type filters.
- [x] Persist download-list history locally.
- [x] Add retry, queue actions, and basic history management in the UI.
- [ ] Verify transfer behavior with real files and network edge cases.
- [ ] Add HTTP range-based resume after interruption or application restart.
- [ ] Add robust temporary-file handling and safe finalization.
- [ ] Add user-selected save locations and destination conflict controls.
- [ ] Add durable task-state persistence and recovery.

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
- [ ] Connect source inspection and format selection to the Flutter workflow and download queue.
- [ ] Add format and quality selection UI.
- [ ] Integrate FFmpeg for audio extraction and MP3 conversion.
- [ ] Build a separate FFmpeg settings/status section and independent update/install lifecycle.
- [ ] Verify FFmpeg official sources, checksums, archive extraction, executable permissions, and rollback behavior.

## Phase 4 — Advanced features

- [ ] Add BitTorrent support.
- [ ] Add browser link handoff and browser integration.
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
