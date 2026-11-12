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

- [ ] Complete yt-dlp integration for supported video sources using a verified, installed executable.
- [x] Add the initial Rust yt-dlp adapter for version probing and single-source HTTP(S) metadata inspection.
- [ ] Connect source inspection to the Flutter workflow and download queue.
- [ ] Add format and quality selection.
- [ ] Integrate FFmpeg for audio extraction and MP3 conversion.
- [ ] Build separate yt-dlp and FFmpeg settings/status sections.
- [ ] Implement independent update checks and safe update/install flows for each tool.
- [ ] Verify checksums, archive extraction, executable permissions, and rollback behavior.
- [ ] Validate compatible yt-dlp runtime packages on Windows, Linux, and Android separately.

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
