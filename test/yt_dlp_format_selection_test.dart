import 'package:flutter_test/flutter_test.dart';
import 'package:easy_download/src/media/yt_dlp_format_selection.dart';

void main() {
  group('YtDlpFormatSelection', () {
    final source = <dynamic>[
      {'formatId': 'best', 'hasVideo': true, 'hasAudio': true},
      {'formatId': 'video-only', 'hasVideo': true, 'hasAudio': false},
      {'formatId': 'audio', 'hasVideo': false, 'hasAudio': true},
      {'formatId': 'empty', 'hasVideo': false, 'hasAudio': false},
      'invalid',
    ];
    test('separates directly playable video from original audio', () {
      expect(YtDlpFormatSelection.videoWithAudio(source).map((f) => f['formatId']), ['best']);
      expect(YtDlpFormatSelection.audioOnly(source).map((f) => f['formatId']), ['audio']);
    });
    test('includes video-only streams for Android FFmpeg merging', () {
      final videos = YtDlpFormatSelection.videoWithMergeSupport(source);
      expect(videos.map((f) => f['formatId']), ['best', 'video-only']);
      expect(YtDlpFormatSelection.byId(videos, 'video-only'), isNotNull);
    });
    test('returns the exact chosen format ID', () {
      final videos = YtDlpFormatSelection.videoWithAudio(source);
      expect(YtDlpFormatSelection.byId(videos, 'best'), videos.single);
      expect(YtDlpFormatSelection.byId(videos, 'video-only'), isNull);
    });
  });
}
