import 'package:flutter_test/flutter_test.dart';
import 'package:easy_download/src/utils/download_helpers.dart';

void main() {
  group('parseHttpLinks', () {
    test('extracts HTTP and HTTPS links from multiple lines', () {
      final links = parseHttpLinks(
        'https://example.com/a.zip\nhttp://example.org/video.mp4',
      );

      expect(links, hasLength(2));
      expect(links.first.toString(), 'https://example.com/a.zip');
      expect(links.last.toString(), 'http://example.org/video.mp4');
    });

    test('removes surrounding punctuation and duplicate links', () {
      final links = parseHttpLinks(
        '(https://example.com/file.zip),\nhttps://example.com/file.zip',
      );

      expect(links, hasLength(1));
      expect(links.single.toString(), 'https://example.com/file.zip');
    });

    test('ignores invalid and unsupported schemes', () {
      expect(parseHttpLinks('file:///tmp/a.zip ftp://example.com/a'), isEmpty);
    });
  });

  group('sanitizeDownloadFileName', () {
    test('replaces both path separators and invalid characters', () {
      expect(
        sanitizeDownloadFileName(r'folder\video?.mp4'),
        'folder_video_.mp4',
      );
    });

    test('uses fallback for empty and dot-only names', () {
      expect(sanitizeDownloadFileName('  '), 'download');
      expect(sanitizeDownloadFileName('..'), 'download');
    });

    test('protects reserved Windows device names', () {
      expect(sanitizeDownloadFileName('CON.txt'), '_CON.txt');
    });

    test('removes trailing periods and spaces', () {
      expect(sanitizeDownloadFileName('video.mp4. '), 'video.mp4');
    });
  });
}
