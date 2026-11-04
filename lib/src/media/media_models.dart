/// Typed contracts for the video/audio download pipeline.
///
/// These models intentionally contain no platform or network implementation.
/// The yt-dlp and FFmpeg adapters will implement the interfaces in later steps.
enum MediaOutputKind { video, audio, file }

enum MediaContainer { mp4, mp3, webm, mkv, m4a, wav, original }

enum ManagedTool { ytDlp, ffmpeg }

enum ToolHealth { unavailable, notInstalled, ready, updateAvailable, updating, failed }

class MediaSourceRequest {
  const MediaSourceRequest({required this.url});

  final Uri url;
}

class MediaFormat {
  const MediaFormat({
    required this.id,
    required this.label,
    required this.outputKind,
    required this.container,
    this.width,
    this.height,
    this.bitrateKbps,
    this.fileSizeBytes,
    this.hasVideo = false,
    this.hasAudio = false,
    this.requiresMerge = false,
  });

  /// Provider-specific format identifier; preserve it exactly when requesting.
  final String id;
  final String label;
  final MediaOutputKind outputKind;
  final MediaContainer container;
  final int? width;
  final int? height;
  final int? bitrateKbps;
  final int? fileSizeBytes;
  final bool hasVideo;
  final bool hasAudio;
  final bool requiresMerge;

  String get resolutionLabel => height == null ? 'Audio / unknown quality' : '${height}p';
}

class MediaInspection {
  const MediaInspection({
    required this.source,
    required this.title,
    required this.formats,
  });

  final Uri source;
  final String title;
  final List<MediaFormat> formats;
}

class MediaTaskRequest {
  const MediaTaskRequest({
    required this.source,
    required this.format,
    required this.outputPath,
  });

  final Uri source;
  final MediaFormat format;
  final String outputPath;
}

class ManagedToolStatus {
  const ManagedToolStatus({
    required this.tool,
    required this.health,
    this.installedVersion,
    this.availableVersion,
    this.platformArtifact,
    this.lastError,
  });

  final ManagedTool tool;
  final ToolHealth health;
  final String? installedVersion;
  final String? availableVersion;
  final String? platformArtifact;
  final String? lastError;

  bool get canUpdate => health == ToolHealth.updateAvailable;
  bool get isReady => health == ToolHealth.ready || canUpdate;
}

/// Contract for source inspection. A concrete yt-dlp adapter will implement it.
abstract interface class MediaSourceInspector {
  Future<MediaInspection> inspect(MediaSourceRequest request);
}

/// Contract for media processing. A concrete FFmpeg adapter will implement it.
abstract interface class MediaProcessor {
  Future<void> process(MediaTaskRequest request);
}

/// Independent lifecycle contract for each bundled/provisioned executable.
abstract interface class ManagedToolController {
  Future<ManagedToolStatus> status(ManagedTool tool);
  Future<ManagedToolStatus> update(ManagedTool tool);
  Future<bool> rollback(ManagedTool tool);
}
