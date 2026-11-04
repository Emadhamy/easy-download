# Video Download Engine Architecture

This document defines the implementation boundary for video capture, format selection, downloading, conversion, and independently managed tool updates.

## Product surfaces

- **Main app**: one download library with distinct item presentation for ordinary files, video (for example MP4), and audio (for example MP3).
- **Floating capture surface**: a browser/desktop overlay that detects a supported media URL and sends it to the app. It is not the download manager screen.
- **Video task flow**: inspect source -> list available formats -> user selects video/audio/container -> enqueue task -> show progress and output.
- **Tool management**: settings screen for yt-dlp and FFmpeg, with separate versions, availability, update status, and rollback state.

## Component boundaries

```text
Flutter UI
  ├─ Download library (file/video/audio item types)
  ├─ Floating capture UI / browser integration
  ├─ Format picker and task creation
  └─ Tool manager UI
       │ typed platform interface
Rust download core
  ├─ Media source inspection adapter (yt-dlp)
  ├─ Download/task orchestration
  ├─ Post-processing adapter (FFmpeg)
  └─ Tool updater / version registry
       └─ platform-specific tool packages
```

The UI must not invoke yt-dlp or FFmpeg directly. All operations go through a typed Rust/platform boundary so the tools can be replaced or updated without changing UI code.

## Task and format model

A media task should retain at least: source URL, selected format identifier, output kind (`video`, `audio`, or `file`), output container/extension, quality label, destination, task state, byte progress, and error details. Format IDs are source-specific and must be passed through unchanged; never assume every source offers MP4, MP3, or every resolution.

- Video output: choose a compatible video+audio stream or a combined stream; merge when required.
- Audio output: acquire the best suitable audio stream and convert to MP3 only when requested and supported.
- Other formats: expose only formats reported by inspection and supported by the installed processing tools.

## Independent tool lifecycle

Treat yt-dlp and FFmpeg as separate managed components, each with its own version, platform/architecture package, checksum/signature metadata, install directory, and update result. Keep the currently active version until a candidate is downloaded and verified. Activate by switching a version pointer/manifest atomically; retain the previous version for rollback. Never overwrite a working executable in place.

The updater must:

1. Obtain release metadata from a configured trusted source.
2. Select an artifact matching OS and CPU architecture.
3. Download to a staging directory.
4. Verify the artifact using trusted checksum/signature data.
5. Check the executable/version response where supported.
6. Activate the candidate only after verification.
7. Preserve the previous version and report failures clearly.

Updates should be user-triggered initially, with a visible version/status screen. Automatic updates can be added later behind a setting. Network failures or failed verification must leave the current version active.

## Platform notes

- **Windows/Linux**: package or provision platform-specific executables and manage their permissions/paths.
- **Android**: use compatible Android builds and app-private storage; do not assume desktop binaries or browser-overlay behavior are available. Android integration and permissions need their own implementation and validation.
- Keep platform-specific acquisition and launching behind the same interface. Do not claim a platform is supported until its artifact and end-to-end flow are tested.

## Implementation sequence

1. Add typed models/interfaces for source inspection, formats, media tasks, and tool status.
2. Build the format-picker and tool-management screens with mock data, clearly labeled as not yet connected.
3. Implement yt-dlp adapter and format inspection for supported sources.
4. Connect the capture surface to task creation and the existing download queue.
5. Implement FFmpeg processing for merge/conversion, including MP3 output.
6. Implement verified, independent update/rollback flows for each tool.
7. Add platform-specific CI and end-to-end tests before marking each platform ready.

## Legal and operational safeguards

Use these tools only for content the user is authorized to download, and respect source terms and applicable law. Keep tool versions and release provenance visible. Avoid silently downloading or executing unverified binaries.
