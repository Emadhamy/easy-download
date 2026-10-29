# Roadmap

## Phase 1 — Foundation
- [x] Create Flutter app scaffold and dark dashboard
- [x] Create initial Rust core crate
- [x] Add basic HTTP(S) URL validation
- [x] Define a typed download queue model (UI/domain only)
- [x] Write unit tests for progress and task state helpers (not yet executed)
- [x] Add GitHub Actions workflow for Flutter and Rust checks (run status not yet confirmed)
- [ ] Run Flutter formatting, analysis, and tests and fix any reported errors
- [ ] Run Rust tests and confirm the toolchain setup
- [x] Choose the bridge approach: `flutter_rust_bridge` v2 (integration still pending)

## Phase 2 — Direct downloads
- [x] Write initial Rust HTTP(S) download function (not yet executed or verified)
- [x] Add chunked response streaming and a progress callback (not yet executed or verified)
- [ ] Generate and integrate `flutter_rust_bridge` bindings for the bridge API
- [ ] Create and manage real download jobs and queue
- [ ] Display progress, speed, and errors in the UI
- [ ] Add pause, resume, cancel, and retry
- [ ] Persist jobs across app restarts
- [ ] Safely finalize partial downloads and handle destination conflicts

## Phase 3 — Platform support
- [ ] Verify Android storage and background execution
- [ ] Verify Windows and Linux file destinations
- [ ] Add platform-specific packaging and CI checks

## Phase 4 — Advanced sources
- [ ] Research a maintainable video extraction integration
- [ ] Add BitTorrent support
- [ ] Add browser integration and link handoff

## Definition of done
A feature is complete only after it builds and has been tested on its target platform. UI placeholders and unverified code do not count as verified functionality.
