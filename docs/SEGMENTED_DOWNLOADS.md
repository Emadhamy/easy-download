# Parallel segmented downloads

Status: approved engineering direction; not implemented yet.

## What exists today

The direct-download engine already streams response data in bounded 64 KiB buffers and supports HTTP Range continuation after an interrupted transfer. It stores one partial file plus validator-aware metadata, then requests the remaining byte range on retry.

That is **single-connection resume**, not parallel multi-segment downloading. The roadmap's older phrase "chunked downloads" must not be interpreted as proof that parallel segments exist.

## Goal

For eligible direct HTTP(S) files, split the known byte length into a small number of independent HTTP byte ranges, download those ranges concurrently, preserve completed ranges across interruptions, and assemble the exact original file only after every range has been verified.

This feature is for direct HTTP(S) downloads first. yt-dlp media downloads and BitTorrent remain separate pipelines until their own transfer semantics are designed and tested.

## Safe capability detection

Do not assume that a server supports ranges merely because it advertises `Accept-Ranges`.

1. Validate and normalize the URL using the existing URL validation path.
2. Probe with `GET` and `Range: bytes=0-0`, requesting identity content encoding.
3. Require a valid `206 Partial Content`, exact `Content-Range: bytes 0-0/TOTAL`, and a known positive total size.
4. Require a stable validator suitable for `If-Range`: a strong ETag, or Last-Modified when a strong ETag is unavailable.
5. If any requirement is absent or inconsistent, use the existing single-connection download/resume path. Never pretend parallel mode is active.

## Initial conservative policy

- Start with 4 concurrent segments per file.
- Cap at 8 segments per file; do not create an unbounded number of workers.
- Use a minimum file-size threshold before segmentation (initial proposal: 8 MiB); smaller files use the existing single-connection path.
- Keep global download concurrency separate from per-file segment concurrency so total sockets remain bounded.
- Make segment count a future user setting only after real-device and desktop measurements. The initial implementation should use safe internal defaults.
- Do not promise a fixed speed multiplier. Server limits, connection quality, disk speed, and host throttling determine actual gains.

## Segment layout and persistence

- Calculate non-overlapping, inclusive byte ranges that cover `0..TOTAL-1` exactly, with no gaps or overlaps.
- Store each segment in its own app-managed temporary part file. This avoids concurrent random writes to one shared file and makes recovery auditable.
- Store an atomic sidecar manifest containing only the request fingerprint, total length, validator, segment plan, and verified segment lengths/status. Never store raw signed URLs, authorization headers, or secrets.
- Fingerprint the normalized request and relevant headers using a cryptographic digest, following the existing resume metadata pattern.
- On restart, retain only segment files whose metadata matches the same request, total length, validator, and exact expected segment range. Discard inconsistent or oversized segment files.
- Write metadata atomically. Flush and sync completed segment data before marking a segment complete.

## Per-segment response validation

Every worker must:
- Send its exact `Range` and `If-Range` headers.
- Require `206 Partial Content`; a `200 OK` response means the server ignored the range and must never be appended as a segment.
- Parse and verify the exact start, end, and total in `Content-Range`.
- Verify the response validator still matches the probed representation.
- Verify the received byte count equals the requested segment length.
- Retry a failed segment a bounded number of times, without resetting already verified segments when the representation is still identical.

If a validator changes, a server returns inconsistent ranges, or the resource changes size, invalidate the whole segment set and restart through the safe single-connection path. Never assemble bytes from different versions of a remote file.

## Pause, cancel, and recovery

- Pause must stop workers at safe read/write boundaries and retain verified segment files and metadata.
- Resume must continue only missing or incomplete ranges after revalidating the remote representation.
- Cancel must stop all workers and remove temporary segment data and metadata according to the app's cancellation policy.
- App/process interruption must preserve already completed segments for a later retry.
- Queue/task state must not claim completion until final assembly and validation succeed.

## Final assembly

1. Confirm every expected segment exists and has its exact expected length.
2. Assemble in byte-range order into a separate staging file.
3. Sync the staging file and verify its final length equals the probed total.
4. Atomically finalize to the destination using the platform-safe replacement rules.
5. Remove segment parts and sidecar metadata only after successful finalization.

If assembly or finalization fails, keep recoverable segment data and do not expose a partial destination as a completed download.

## Required automated tests

Use a deterministic local HTTP test server; do not depend on public websites.

- Correct multi-range responses assemble byte-for-byte identical output.
- Range arithmetic covers tiny boundary cases, uneven divisions, and large lengths without overflow.
- Servers that ignore Range or return malformed/wrong Content-Range never produce a corrupt output.
- Changed ETag/Last-Modified or changed total length invalidates stale segments.
- Short, oversized, interrupted, and failed segments are retried safely.
- A retry downloads only missing ranges when the validator is unchanged.
- Pause/resume preserves verified segments; cancel removes temporary state.
- Concurrent worker count never exceeds the configured cap.
- Progress aggregates completed and in-flight bytes without double counting.
- Disk-full, permission, and final-assembly failures never mark the task complete.
- Existing single-connection resume tests continue to pass.

## Cross-platform plan

The segmented engine belongs in the shared Rust core and must not depend on desktop-only APIs. Exercise it through the existing Flutter-Rust bridge on Windows, Linux, and Android.

macOS is a planned additional desktop target, not currently supported by the managed-tools target registry. Before claiming macOS support, add explicit `macos-x86_64` and `macos-aarch64` target handling, Flutter desktop integration, packaging/signing/notarization work, and CI coverage. Do not map macOS to Linux binaries or infer support from shared Rust code compiling elsewhere.

## Rollout

1. Land this specification and tests for range arithmetic/response validation.
2. Implement the Rust segmented transfer module behind a conservative capability gate.
3. Integrate pause/resume/cancel and aggregate progress with the existing task controls.
4. Run the full CI matrix and deterministic local-server tests.
5. Enable by default only after measured verification on Windows, Linux, and Android; retain single-connection fallback for unsupported servers.
