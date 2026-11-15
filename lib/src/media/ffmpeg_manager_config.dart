/// Supported MP3 output bitrates exposed by the FFmpeg conversion UI.
/// Keep this allowlist aligned with Rust's FFmpeg bitrate validation.
const List<String> ffmpegMp3Bitrates = <String>[
  '128k',
  '160k',
  '192k',
  '256k',
  '320k',
];
