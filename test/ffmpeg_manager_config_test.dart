import 'package:flutter_test/flutter_test.dart';
import 'package:easy_download/src/media/ffmpeg_manager_config.dart';

void main() {
  test('exposes the five supported MP3 output bitrates in ascending order', () {
    expect(ffmpegMp3Bitrates, <String>['128k', '160k', '192k', '256k', '320k']);
    expect(ffmpegMp3Bitrates.toSet(), hasLength(5));
  });
}
