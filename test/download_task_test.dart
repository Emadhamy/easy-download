import 'package:flutter_test/flutter_test.dart';
import 'package:easy_download/models/download_task.dart';

void main() {
  final createdAt = DateTime.utc(2026, 1, 1);

  DownloadTask task({
    int receivedBytes = 0,
    int? totalBytes,
    DownloadStatus status = DownloadStatus.queued,
  }) {
    return DownloadTask(
      id: 'job-1',
      url: 'https://example.com/archive.zip',
      fileName: 'archive.zip',
      status: status,
      createdAt: createdAt,
      receivedBytes: receivedBytes,
      totalBytes: totalBytes,
    );
  }

  group('DownloadTask progress', () {
    test('is unknown when the total size is not known', () {
      expect(task(receivedBytes: 20).progress, isNull);
    });

    test('calculates progress between zero and one', () {
      expect(task(receivedBytes: 25, totalBytes: 100).progress, 0.25);
    });

    test('clamps progress when received bytes exceed the total', () {
      expect(task(receivedBytes: 120, totalBytes: 100).progress, 1.0);
    });

    test('treats zero-sized files as unknown progress', () {
      expect(task(totalBytes: 0).progress, isNull);
    });
  });

  group('DownloadTask state helpers', () {
    test('only a downloading task can be paused', () {
      expect(task(status: DownloadStatus.downloading).canPause, isTrue);
      expect(task(status: DownloadStatus.queued).canPause, isFalse);
    });

    test('only a paused task can be resumed', () {
      expect(task(status: DownloadStatus.paused).canResume, isTrue);
      expect(task(status: DownloadStatus.failed).canResume, isFalse);
    });

    test('finished states are identified correctly', () {
      expect(task(status: DownloadStatus.completed).isFinished, isTrue);
      expect(task(status: DownloadStatus.failed).isFinished, isTrue);
      expect(task(status: DownloadStatus.cancelled).isFinished, isTrue);
      expect(task(status: DownloadStatus.paused).isFinished, isFalse);
    });
  });
}
