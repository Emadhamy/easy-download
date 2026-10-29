# Easy Download

A cross-platform download manager planned with **Flutter** for the UI and **Rust** for core download logic.

## Current status

This repository contains an early starter scaffold:
- A dark Flutter dashboard UI.
- A small Rust crate for validating HTTP(S) URLs.
- Architecture notes and a phased roadmap.

**Important:** real downloads, pause/resume, video extraction, BitTorrent, browser integration, and the Flutter–Rust bridge are not implemented yet. The starter has not been compiled or tested in this repository, so run the checks below on a machine with the required toolchains before relying on it.

## Project layout

- `lib/` — Flutter UI
- `rust/` — Rust core utilities
- `docs/ARCHITECTURE.md` — architecture and boundaries
- `docs/ROADMAP.md` — development plan

## Development checks

Flutter (from the repository root):
```sh
flutter pub get
flutter analyze
flutter test
```

Rust (from the `rust/` directory):
```sh
cargo test
```

See [the roadmap](docs/ROADMAP.md) for planned work.
