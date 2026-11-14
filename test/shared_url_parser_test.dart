import 'package:flutter_test/flutter_test.dart';
import 'package:easy_download/src/utils/shared_url_parser.dart';

void main() {
  group('extractSharedHttpUrl', () {
    test('extracts a URL from shared text', () {
      expect(extractSharedHttpUrl('Watch this: https://example.com/video?id=2'),
          Uri.parse('https://example.com/video?id=2'));
    });
    test('removes common trailing punctuation', () {
      expect(extractSharedHttpUrl('https://example.com/video).'),
          Uri.parse('https://example.com/video'));
    });
    test('rejects arbitrary text and non-web schemes', () {
      expect(extractSharedHttpUrl('just a title'), isNull);
      expect(extractSharedHttpUrl('file:///storage/video.mp4'), isNull);
      expect(extractSharedHttpUrl(null), isNull);
    });
  });
}
