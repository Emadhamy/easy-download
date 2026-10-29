/// Domain model for one item in the Easy Download queue.
///
/// This model describes download state only. It does not start or manage a
/// network transfer; that belongs to the download engine.
enum DownloadStatus {
  queued,
  downloading,
  paused,
  completed,
  failed,
  cancelled,
}

class DownloadTask {
  const DownloadTask({
    required this.id,
    required this.url,
    required this.fileName,
    required this.status,
    required this.createdAt,
    this.receivedBytes = 0,
    this.totalBytes,
    this.speedBytesPerSecond = 0,
    this.errorMessage,
  })  : assert(receivedBytes >= 0),
        assert(totalBytes == null || totalBytes >= 0),
        assert(speedBytesPerSecond >= 0);

  final String id;
  final String url;
  final String fileName;
  final DownloadStatus status;
  final DateTime createdAt;
  final int receivedBytes;
  final int? totalBytes;
  final int speedBytesPerSecond;
  final String? errorMessage;

  /// A value from 0.0 to 1.0, or null when the server has not reported size.
  double? get progress {
    final total = totalBytes;
    if (total == null || total <= 0) return null;
    return (receivedBytes / total).clamp(0.0, 1.0).toDouble();
  }

  bool get canPause => status == DownloadStatus.downloading;
  bool get canResume => status == DownloadStatus.paused;
  bool get isFinished =>
      status == DownloadStatus.completed ||
      status == DownloadStatus.failed ||
      status == DownloadStatus.cancelled;

  DownloadTask copyWith({
    String? fileName,
    DownloadStatus? status,
    int? receivedBytes,
    int? totalBytes,
    bool clearTotalBytes = false,
    int? speedBytesPerSecond,
    String? errorMessage,
    bool clearErrorMessage = false,
  }) {
    return DownloadTask(
      id: id,
      url: url,
      fileName: fileName ?? this.fileName,
      status: status ?? this.status,
      createdAt: createdAt,
      receivedBytes: receivedBytes ?? this.receivedBytes,
      totalBytes: clearTotalBytes ? null : (totalBytes ?? this.totalBytes),
      speedBytesPerSecond: speedBytesPerSecond ?? this.speedBytesPerSecond,
      errorMessage:
          clearErrorMessage ? null : (errorMessage ?? this.errorMessage),
    );
  }
}
