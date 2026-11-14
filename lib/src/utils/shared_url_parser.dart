/// Safely extracts the first HTTP(S) URL from Android's shared text.
Uri? extractSharedHttpUrl(String? sharedText) {
  if (sharedText == null || sharedText.trim().isEmpty) return null;
  final matches = RegExp(r'https?://[^\s<>"\u0000-\u001F]+', caseSensitive: false).allMatches(sharedText);
  for (final match in matches) {
    var candidate = match.group(0)!.trim();
    candidate = candidate.replaceFirst(RegExp(r'[),.;!?\]}]+$'), '');
    final uri = Uri.tryParse(candidate);
    if (uri != null &&
        (uri.scheme == 'http' || uri.scheme == 'https') &&
        uri.host.isNotEmpty &&
        !uri.host.contains(' ')) {
      return uri;
    }
  }
  return null;
}
