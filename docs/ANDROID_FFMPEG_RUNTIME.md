# Android FFmpeg runtime decision

Status: design decision; no Android FFmpeg binary or dependency is enabled yet.

## Current application boundary

- The Android Gradle app currently depends on the `youtubedl-android:library:0.18.1` module only. It does not include the upstream FFmpeg AAR.
- The Flutter FFmpeg manager intentionally supports Windows and Linux only. Android MP3 conversion remains unavailable until a separate Android runtime is built, verified, installed, and exercised on devices.
- The desktop BtbN updater and installer reject Android targets. A Linux executable must never be treated as an Android executable.

## Rejected shortcut: upstream FFmpeg AAR 0.18.1

Do not add `io.github.junkfood02.youtubedl-android:ffmpeg:0.18.1` as a production dependency without a verified upstream fix and a complete ABI audit.

The upstream repository currently lists two relevant open issues:

- Issue #369 reports that `librubberband.so` in the 0.18.1 payload requires `libc++_shared.so`, which is absent from the published AAR and its embedded payload. The reported failure occurs at Android dynamic-link time and can prevent FFmpeg from starting.
- Issue #365 reports that several shared libraries nested inside `libffmpeg.zip.so` still have 4 KB ELF segment alignment and fail on 16 KB page-size Android devices, even though some top-level libraries were rebuilt for 16 KB alignment.

These are runtime/linker failures. A successful Gradle build or APK packaging check cannot establish that the FFmpeg process will run. Do not treat `pickFirst` packaging rules as a fix for a missing or ABI-incompatible native library.

References:
- https://github.com/yausername/youtubedl-android/issues/369
- https://github.com/yausername/youtubedl-android/issues/365
- https://github.com/yausername/youtubedl-android

## Preferred direction: independently built Android executable

Evaluate a reproducible Android NDK build of FFmpeg as a standalone app-private executable, rather than coupling FFmpeg to the yt-dlp Android AAR.

Initial build target:
- `arm64-v8a` / Rust target `aarch64-linux-android`
- Keep other Android ABIs explicitly unsupported until their own artifacts and tests exist.
- Use Android API 24 as the minimum platform to match the app's current minimum SDK.

The build must:
1. Pin FFmpeg, LAME, NDK, build scripts, and all source archive digests to immutable versions.
2. Enable the MP3 encoder required by the existing Rust conversion adapter (`libmp3lame`); verify the encoder is present in the resulting executable.
3. Avoid optional libraries that are not required by Easy Download, reducing native dependency and attack surface.
4. Produce an Android-compatible executable and audit every ELF dependency with the NDK's `llvm-readelf`.
5. Require 16 KB-compatible ELF load-segment alignment for every shipped ELF file.
6. Run a bounded `ffmpeg -version` probe and an encoder-capability probe before an artifact can be published.
7. Include a known, redistributable media fixture and run an actual MP3 conversion smoke test on an Android emulator/device for each supported ABI.

The final build configuration must be reviewed for GPL-3.0 compatibility and its complete third-party notices must be shipped with the runtime.

## Independent updater contract

Android FFmpeg must remain independent from yt-dlp:
- Its version, artifact, checksum, installation directory, update history, and rollback pointer are separate.
- Resolve artifacts only from a project-controlled release manifest with exact ABI, version, HTTPS URL, and SHA-256.
- Download to staging, verify before activation, install into a versioned app-private directory, and preserve the previous known-good version until the new executable passes runtime probes.
- Failed downloads, checksum mismatches, linker failures, and failed conversion smoke tests must leave the active version unchanged.
- Never silently fall back to a Linux desktop binary or an unverified third-party mirror.

## Acceptance gate before enabling Android MP3

- [ ] Reproducible NDK build and pinned source/toolchain inputs.
- [ ] Artifact and ELF dependency audit for every supported ABI.
- [ ] 16 KB ELF alignment audit, including every shipped shared object.
- [ ] `ffmpeg -version` succeeds on the minimum supported Android API and a current 16 KB page-size environment.
- [ ] `ffmpeg -encoders` reports `libmp3lame`.
- [ ] Real fixture conversion produces a non-empty, decodable MP3 at each supported bitrate.
- [ ] App-private install, independent update, failed-update preservation, and rollback tests pass.
- [ ] Real-device test confirms conversion does not crash the app process.

Until all gates pass, Android must continue to report FFmpeg as unavailable and must not expose a working-looking MP3 conversion action.
