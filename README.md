# Easy Download

A cross-platform download manager built with **Flutter** for the interface and **Rust** for download operations. The project targets Windows, Linux, and Android.

## Current implementation

The repository is beyond the initial scaffold. The current code includes:

- A dark Flutter download dashboard.
- Direct HTTP(S) URL validation through the Rust API.
- A Flutter–Rust bridge generated with `flutter_rust_bridge` v2.
- Direct file downloads with streamed progress events.
- Pause, resume, and cancel controls for active downloads.
- A local download history stored with `shared_preferences`.
- Search, file-type filters, sorting, selection, and clipboard actions.
- A managed-tools manifest structure for yt-dlp and FFmpeg.
- An initial Rust yt-dlp adapter for executable version probing and single-source HTTP(S) metadata inspection (not media downloading).

**Verification note:** code being present does not mean every feature has been verified on every target. GitHub Actions is used to check the current Rust and Flutter changes; platform-specific behavior still requires testing on the actual target devices. See [the roadmap](docs/ROADMAP.md) and [architecture notes](docs/ARCHITECTURE.md) for implementation boundaries and known gaps.

## Not implemented yet

- Resumable downloads that continue from previously downloaded partial data.
- Reliable background downloads and platform-specific storage handling.
- End-to-end video downloading through yt-dlp, including verified executable provisioning and Flutter queue integration.
- Android yt-dlp runtime support, pending a compatible and tested package.
- MP3 conversion through FFmpeg.
- BitTorrent support.
- Browser integration and floating video capture.
- Independent in-app downloading and updating of yt-dlp and FFmpeg.
- Production installers and release packaging for Windows, Linux, and Android.

The yt-dlp/FFmpeg manifest is intentionally **unconfigured**. No executable is downloaded or launched by the current managed-tools code.

## Project layout

- `lib/` — Flutter UI and download-list behavior.
- `lib/src/rust/` — generated Dart bindings for the Rust API.
- `rust/` — Rust download core, bridge API, and managed-tools metadata validation.
- `docs/ARCHITECTURE.md` — current architecture and implementation boundaries.
- `docs/ROADMAP.md` — phased development plan.
- `.github/workflows/` — automated checks.

## Development checks

From the repository root:

```sh
flutter pub get
flutter_rust_bridge_codegen generate
flutter analyze
flutter test
```

Rust checks:

```sh
cargo test --manifest-path rust/Cargo.toml
```

Android builds and Windows/Linux packaging require their respective platform toolchains. A feature is considered complete only after the relevant build and tests have actually passed on its target platform.
