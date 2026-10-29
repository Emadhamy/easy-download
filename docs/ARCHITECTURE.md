# Architecture (initial)

Easy Download is planned as a Flutter application with a Rust core.

- `lib/`: Flutter presentation layer and, later, download queue UI.
- `rust/`: Rust core utilities. At this stage it only validates HTTP(S) URLs.
- A Flutter-to-Rust bridge has **not** been added yet.
- Direct downloading, pause/resume, video extraction, BitTorrent, browser integrations, and platform packaging are **not implemented yet**.

## Planned boundaries

1. Flutter owns screens, user interaction, and platform-specific presentation.
2. Rust will own download jobs, network transfers, progress reporting, and torrent functionality where appropriate.
3. A typed bridge will communicate job creation, pause/resume/cancel, and progress events between Flutter and Rust.
4. Platform capabilities and storage permissions must be handled explicitly per target OS.

Keep this document updated as implementation decisions are tested.
