/// Pure filtering and lookup helpers for the Android yt-dlp format picker.
class YtDlpFormatSelection {
  static List<Map<String, dynamic>> videoWithAudio(Iterable<dynamic> formats) =>
      formats.whereType<Map>()
          .map((format) => Map<String, dynamic>.from(format))
          .where((format) => format['hasVideo'] == true && format['hasAudio'] == true)
          .toList();

  /// Video streams suitable for Android, including video-only streams that
  /// yt-dlp can merge with the best available audio using its bundled FFmpeg.
  static List<Map<String, dynamic>> videoWithMergeSupport(Iterable<dynamic> formats) =>
      formats.whereType<Map>()
          .map((format) => Map<String, dynamic>.from(format))
          .where((format) => format['hasVideo'] == true)
          .toList();

  static List<Map<String, dynamic>> audioOnly(Iterable<dynamic> formats) =>
      formats.whereType<Map>()
          .map((format) => Map<String, dynamic>.from(format))
          .where((format) => format['hasVideo'] != true && format['hasAudio'] == true)
          .toList();

  static Map<String, dynamic>? byId(Iterable<Map<String, dynamic>> formats, String formatId) {
    for (final format in formats) {
      if (format['formatId'] == formatId) return format;
    }
    return null;
  }
}
