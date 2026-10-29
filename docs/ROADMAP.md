# Roadmap

## Phase 1 — Foundation
- [x] Create Flutter app scaffold and dark dashboard
- [x] Create initial Rust core crate
- [x] Add basic HTTP(S) URL validation
- [x] Define a typed download queue model (UI/domain only)
- [x] Write unit tests for progress and task state helpers (not yet executed)
- [ ] Run Flutter formatting, analysis, and tests on a development machine
- [ ] Run Rust tests and confirm the toolchain setup
- [ ] Choose and implement the Flutter–Rust bridge

## Phase 2 — Direct downloads
- [ ] Create a download job model and queue
- [ ] Implement real HTTP(S) file downloads in Rust
- [ ] Report progress, speed, and errors
- [ ] Add pause, resume, cancel, and retry
- [ ] Persist jobs across app restarts

## Phase 3 — Platform support
- [ ] Verify Android storage and background execution
- [ ] Verify Windows and Linux file destinations
- [ ] Add platform-specific packaging and CI checks

## Phase 4 — Advanced sources
- [ ] Research a maintainable video extraction integration
- [ ] Add BitTorrent support
- [ ] Add browser integration and link handoff

## Definition of done
A feature is complete only after it builds and has been tested on its target platform. UI placeholders do not count as implemented functionality.
