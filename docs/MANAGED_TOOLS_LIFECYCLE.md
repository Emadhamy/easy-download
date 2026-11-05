# Managed video tools: lifecycle contract

This document defines the security and lifecycle boundary for the independently managed
`yt-dlp` and `FFmpeg` executables. It is a design contract, not an implemented updater.

## Required invariants

- Manage each tool independently; updating one must not replace or invalidate the other.
- Never execute an artifact directly from its download or staging location.
- Resolve an artifact only from a trusted, versioned release manifest maintained by the project.
- The manifest must identify the exact version, supported target, download URL, archive format,
  expected SHA-256 digest, and executable path inside the archive.
- Accept HTTPS downloads only. Reject redirects to non-HTTPS destinations.
- Verify the complete downloaded artifact against the manifest digest before extraction.
- Extract only the expected executable. Reject absolute paths, parent-directory traversal,
  symlinks, and unexpected archive entries.
- Stage and validate the candidate in a temporary directory on the same filesystem as the
  managed-tools directory.
- Activate by atomic rename/swap where supported. Keep the previous known-good version until
  the candidate passes a version/probe check.
- If activation or probing fails, restore the previous version and report a structured error.
- Rollback must target a previously verified version; never delete the active version first.
- Serialize update/rollback operations per tool. A failure for one tool must not block the other.
- Do not require administrator/root privileges; use an app-owned data directory.
- Never mark a tool as ready merely because a file exists. Verify executable permissions and
  successfully run a bounded version probe.

## Suggested state transitions

`notInstalled -> downloading -> verifying -> staging -> probing -> ready`

`ready -> updateAvailable -> downloading -> verifying -> staging -> probing -> ready`

Any failed transition returns to the prior known-good state and records a safe, user-readable
error. If no known-good version exists, report `failed` or `notInstalled` as appropriate.

## Platform artifact selection

Select artifacts using an explicit target tuple (operating system, CPU architecture, and
supported ABI where applicable). Unknown targets must fail closed; do not guess a download URL
or silently substitute another architecture. Android support must be assessed separately because
executable and background-process restrictions differ from desktop platforms.

## Implementation sequence

1. Add a typed manifest and target-selection model.
2. Add a downloader that writes only to a temporary staging path.
3. Add digest verification and safe archive extraction.
4. Add atomic activation, version probing, and rollback.
5. Implement `ManagedToolController` against this lifecycle.
6. Add tests for digest mismatch, unsafe archive paths, interrupted downloads, failed probes,
   rollback, and independent updates.
7. Connect the controller to the Flutter managed-tools screen only after the Rust tests pass.

## Scope note

This contract does not yet provide release URLs, trusted checksums, artifact selection, or an
operational updater. Those must be sourced and reviewed before executable downloads are enabled.
