/// Small, deterministic helpers shared by the download UI and its tests.

/// Extracts unique HTTP(S) URLs from text, preserving their original order.
List<Uri> parseHttpLinks(String text) {
  final seen = <String>{};
  final links = <Uri>[];

  for (final token in text.split(RegExp(r'\s+'))) {
    final candidate = token
        .trim()
        .replaceAll(RegExp(r'^[<("]+|[>)",]+$'), '');
    if (candidate.isEmpty) continue;

    final uri = Uri.tryParse(candidate);
    if (uri == null ||
        (uri.scheme != 'http' && uri.scheme != 'https') ||
        uri.host.isEmpty) {
      continue;
    }

    final normalized = uri.toString();
    if (seen.add(normalized)) links.add(uri);
  }

  return links;
}

/// Produces a portable file name that cannot escape the chosen download folder.
String sanitizeDownloadFileName(
  String input, {
  String fallback = 'download',
}) {
  var cleaned = input
      .replaceAll(RegExp(r'[/\\<>:"|?*\x00-\x1F]'), '_')
      .trim()
      .replaceFirst(RegExp(r'[. ]+$'), '');

  if (cleaned.isEmpty || cleaned == '.' || cleaned == '..') {
    cleaned = fallback;
  }

  // Windows reserves these device names, even when an extension is present.
  final firstSegment = cleaned.split('.').first.toUpperCase();
  const reservedNames = {
    'CON', 'PRN', 'AUX', 'NUL',
    'COM1', 'COM2', 'COM3', 'COM4', 'COM5',
    'COM6', 'COM7', 'COM8', 'COM9',
    'LPT1', 'LPT2', 'LPT3', 'LPT4', 'LPT5',
    'LPT6', 'LPT7', 'LPT8', 'LPT9',
  };
  if (reservedNames.contains(firstSegment)) {
    cleaned = '_$cleaned';
  }

  return cleaned;
}
