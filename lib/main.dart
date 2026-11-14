import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:file_picker/file_picker.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'dart:convert';
import 'dart:io';
import 'src/utils/download_helpers.dart';
import 'src/utils/shared_url_parser.dart';
import 'src/storage/download_history_store.dart';
import 'src/theme/easy_download_theme.dart';
import 'src/rust/api.dart' as rust_api;
import 'src/rust/frb_generated.dart' show RustLib;

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  runApp(const EasyDownloadApp());
}

class EasyDownloadApp extends StatelessWidget {
  const EasyDownloadApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Easy Download',
      debugShowCheckedModeBanner: false,
      theme: EasyDownloadTheme.dark,
      home: const DownloadHomePage(),
    );
  }
}

String _fileType(String name) {
  final extension = name.split('.').last.toLowerCase();
  if (['mp4', 'mkv', 'mov', 'webm', 'avi'].contains(extension)) return 'Video';
  if (['mp3', 'wav', 'flac', 'm4a', 'ogg'].contains(extension)) return 'Audio';
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg'].contains(extension)) return 'Images';
  if (['zip', 'rar', '7z', 'tar', 'gz'].contains(extension)) return 'Archives';
  if (['pdf', 'doc', 'docx', 'txt', 'xls', 'xlsx', 'ppt', 'pptx'].contains(extension)) return 'Documents';
  return 'Other';
}

IconData _downloadIconForName(String name) {
  final extension = name.split('.').last.toLowerCase();
  if (['mp4', 'mkv', 'mov', 'webm', 'avi'].contains(extension)) return Icons.movie_outlined;
  if (['mp3', 'wav', 'flac', 'm4a', 'ogg'].contains(extension)) return Icons.audio_file_outlined;
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg'].contains(extension)) return Icons.image_outlined;
  if (['zip', 'rar', '7z', 'tar', 'gz'].contains(extension)) return Icons.folder_zip_outlined;
  if (extension == 'pdf') return Icons.picture_as_pdf_outlined;
  return Icons.insert_drive_file_outlined;
}

Color _downloadAccentForName(String name) {
  switch (_fileType(name)) {
    case 'Video':
      return EasyDownloadPalette.accent;
    case 'Audio':
      return EasyDownloadPalette.accentSoft;
    case 'Images':
      return const Color(0xFF73B7FF);
    case 'Archives':
      return const Color(0xFFB69AFF);
    case 'Documents':
      return const Color(0xFF80C9B0);
    default:
      return EasyDownloadPalette.muted;
  }
}

String _downloadTypeLabel(String name) {
  final extension = name.contains('.') ? name.split('.').last.toUpperCase() : '';
  if (extension.isEmpty || extension.length > 5) return _fileType(name).toUpperCase();
  return extension;
}

class _LinkDraft {
  _LinkDraft({required this.url, required this.fileName});
  final Uri url;
  String fileName;
  bool isDownloading = false;
  bool isPaused = false;
  bool isCancelled = false;
  bool isFailed = false;
  String? taskId;
  int receivedBytes = 0;
  int? totalBytes;
  double? progressPercent;
  int? progressEtaSeconds;
  double speedBytesPerSecond = 0;
  String? savedPath;
  String? pendingPath;
  String? ytDlpFormatId;

  Map<String, dynamic> toJson() => {
        'url': url.toString(),
        'fileName': fileName,
        'savedPath': savedPath,
        'pendingPath': pendingPath,
        'ytDlpFormatId': ytDlpFormatId,
        'isFailed': isFailed,
        'isCancelled': isCancelled,
        'receivedBytes': receivedBytes,
        'totalBytes': totalBytes,
      };

  static _LinkDraft? fromJson(Map<String, dynamic> json) {
    final parsed = Uri.tryParse(json['url'] as String? ?? '');
    if (parsed == null || !parsed.hasScheme || parsed.host.isEmpty) return null;
    return _LinkDraft(url: parsed, fileName: json['fileName'] as String? ?? parsed.host)
      ..savedPath = json['savedPath'] as String?
      ..pendingPath = json['pendingPath'] as String?
      ..ytDlpFormatId = json['ytDlpFormatId'] as String?
      ..isFailed = json['isFailed'] as bool? ?? false
      ..isCancelled = json['isCancelled'] as bool? ?? false
      ..receivedBytes = (json['receivedBytes'] as num?)?.toInt() ?? 0
      ..totalBytes = (json['totalBytes'] as num?)?.toInt();
  }
}

class _VideoFormatSelection {
  const _VideoFormatSelection({
    required this.sourceUrl,
    required this.title,
    required this.formatId,
    required this.extension,
  });

  final Uri sourceUrl;
  final String title;
  final String formatId;
  final String extension;
}

class DownloadHomePage extends StatefulWidget {
  const DownloadHomePage({super.key});

  @override
  State<DownloadHomePage> createState() => _DownloadHomePageState();
}

class _DownloadHomePageState extends State<DownloadHomePage> {
  final List<_LinkDraft> _links = [];
  String _filter = 'All';
  String _typeFilter = 'All';
  String _searchQuery = '';
  String _sortBy = 'Newest';
  bool _showOnlyFailed = false;
  bool _showOnlyPaused = false;
  final Set<_LinkDraft> _selectedLinks = {};
  bool _selectionMode = false;
  static const _historyKey = 'easy_download_history_v1';
  DownloadHistoryStore? _historyStore;

  Future<DownloadHistoryStore> _getHistoryStore() async {
    final existing = _historyStore;
    if (existing != null) return existing;
    final support = await getApplicationSupportDirectory();
    return _historyStore ??= DownloadHistoryStore(File(
      '${support.path}${Platform.pathSeparator}download-history-v2.json',
    ));
  }
  static const _downloadDirectoryKey = 'easy_download_directory_v1';
  static const _androidDirectoryKey = 'easy_download_android_tree_uri_v1';
  static const _storageChannel = MethodChannel('com.easydownload/storage');
  static const _shareIntentsChannel = MethodChannel('com.easydownload/share_intents');
  static const _ytDlpAndroidChannel = MethodChannel('com.easydownload/ytdlp_android');
  String? _downloadDirectoryPath;
  String? _androidDownloadTreeUri;
  static const int _maxConcurrentDownloads = 3;
  static const _ytDlpAutoCheckKey = 'easy_download_yt_dlp_auto_check_v1';
  static const _ytDlpLastCheckKey = 'easy_download_yt_dlp_last_check_v1';
  static const _ytDlpHistoryKey = 'easy_download_yt_dlp_history_v1';

  Timer? _ytDlpUpdateTimer;
  Future<void>? _ytDlpInitialization;
  bool _ytDlpAutoCheckEnabled = true;
  bool _ytDlpChecking = false;
  bool _ytDlpInstalling = false;
  bool _ytDlpRollingBack = false;
  String? _ytDlpInstalledVersion;
  String? _ytDlpLatestVersion;
  String? _ytDlpLastCheckedAt;
  String _ytDlpStatus = 'Preparing yt-dlp manager...';
  int _ytDlpReceivedBytes = 0;
  int? _ytDlpTotalBytes;
  List<String> _ytDlpHistory = [];

  static const _ffmpegExecutableKey = 'easy_download_ffmpeg_executable_v1';
  static const _ffmpegAutoCheckKey = 'easy_download_ffmpeg_auto_check_v1';
  static const _ffmpegLastCheckKey = 'easy_download_ffmpeg_last_check_v1';
  static const _ffmpegHistoryKey = 'easy_download_ffmpeg_history_v1';
  Timer? _ffmpegUpdateTimer;
  bool _ffmpegAutoCheckEnabled = true;
  bool _ffmpegChecking = false;
  bool _ffmpegInstalling = false;
  String? _ffmpegLatestVersion;
  String? _ffmpegLastCheckedAt;
  int _ffmpegReceivedBytes = 0;
  int? _ffmpegTotalBytes;
  List<String> _ffmpegHistory = [];
  String? _ffmpegExecutablePath;
  String? _ffmpegVersion;
  String? _ffmpegInputPath;
  String _ffmpegBitrate = '192k';
  String _ffmpegStatus = 'Choose a verified FFmpeg executable to enable conversion.';
  bool _ffmpegConverting = false;
  bool _ffmpegPaused = false;
  bool _ffmpegCancelRequested = false;
  String? _ffmpegTaskId;
  int _ffmpegElapsedMillis = 0;
  int? _ffmpegTotalMillis;

  @override
  void initState() {
    super.initState();
    _loadHistory();
    _ytDlpAndroidChannel.setMethodCallHandler((call) async {
      if (call.method == 'downloadProgress' && call.arguments is Map) {
        final event = Map<dynamic, dynamic>.from(call.arguments as Map);
        final taskId = event['taskId'] as String?;
        final percent = (event['percent'] as num?)?.toDouble();
        if (taskId == null || percent == null || !mounted) return;
        final matching = _links.where((item) => item.taskId == taskId).toList();
        if (matching.isEmpty) return;
        setState(() {
          matching.first.progressPercent = percent.clamp(0.0, 100.0);
          matching.first.progressEtaSeconds = (event['etaSeconds'] as num?)?.toInt();
        });
      }
    });
    _shareIntentsChannel.setMethodCallHandler((call) async {
      if (call.method == "sharedContentReceived") {
        await _handleSharedContent(call.arguments as String?);
        await _shareIntentsChannel.invokeMethod<void>("acknowledgeSharedContent");
      }
    });
    unawaited(_loadInitialSharedContent());
    _ytDlpInitialization = _initializeYtDlpManager();
    unawaited(_initializeFfmpegManager());
  }

  @override
  void dispose() {
    _ytDlpUpdateTimer?.cancel();
    _ffmpegUpdateTimer?.cancel();
    super.dispose();
  }


  Future<void> _loadInitialSharedContent() async {
    try {
      final shared = await _shareIntentsChannel.invokeMethod<String>('getInitialSharedContent');
      if (shared != null) await _handleSharedContent(shared);
    } on PlatformException catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Could not receive shared link: ${error.message ?? error.code}')),
        );
      }
    }
  }

  Future<void> _handleSharedContent(String? sharedText) async {
    final uri = extractSharedHttpUrl(sharedText);
    if (!mounted) return;
    if (uri == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('The shared content does not contain a valid HTTP(S) link.')),
      );
      return;
    }
    if (Platform.isAndroid && _ytDlpInstalledVersion == null) {
      await _ytDlpInitialization;
      if (!mounted) return;
    }
    await _addVideoLink(initialUrl: uri.toString());
  }

  bool get _ytDlpSupported => Platform.isWindows || Platform.isLinux;
  bool get _ytDlpManagerSupported => _ytDlpSupported || Platform.isAndroid;

  Future<String> _ytDlpInstallRoot() async {
    final support = await getApplicationSupportDirectory();
    return '${support.path}${Platform.pathSeparator}managed-tools${Platform.pathSeparator}yt-dlp';
  }

  Future<void> _initializeYtDlpManager() async {
    if (!_ytDlpManagerSupported) {
      if (mounted) setState(() => _ytDlpStatus = 'yt-dlp is not supported on this platform.');
      return;
    }

    final prefs = await SharedPreferences.getInstance();
    if (!mounted) return;
    setState(() {
      _ytDlpAutoCheckEnabled = prefs.getBool(_ytDlpAutoCheckKey) ?? true;
      _ytDlpLastCheckedAt = prefs.getString(_ytDlpLastCheckKey);
      _ytDlpHistory = prefs.getStringList(_ytDlpHistoryKey) ?? [];
    });

    if (Platform.isAndroid) {
      try {
        final runtime = await _ytDlpAndroidChannel.invokeMapMethod<String, dynamic>('initialize');
        if (mounted) {
          setState(() {
            _ytDlpInstalledVersion = runtime?['version'] as String?;
            _ytDlpStatus = 'Android yt-dlp runtime initialized. The official updater is ready.';
          });
        }
      } catch (error) {
        if (mounted) setState(() => _ytDlpStatus = 'Android yt-dlp initialization failed: $error');
        return;
      }
    } else {
      await _refreshInstalledYtDlp();
    }
    if (_ytDlpAutoCheckEnabled) {
      await _checkYtDlpUpdates(silent: true);
      _scheduleYtDlpChecks();
    } else if (mounted) {
      setState(() => _ytDlpStatus = 'Automatic update checks are paused.');
    }
  }

  void _scheduleYtDlpChecks() {
    _ytDlpUpdateTimer?.cancel();
    if (!_ytDlpManagerSupported || !_ytDlpAutoCheckEnabled) return;
    _ytDlpUpdateTimer = Timer.periodic(const Duration(hours: 24), (_) {
      unawaited(_checkYtDlpUpdates(silent: true));
    });
  }

  Future<void> _setYtDlpAutoCheck(bool enabled) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_ytDlpAutoCheckKey, enabled);
    if (!mounted) return;
    setState(() {
      _ytDlpAutoCheckEnabled = enabled;
      _ytDlpStatus = enabled
          ? 'Automatic checks enabled (every 24 hours while the app is open).'
          : 'Automatic update checks are paused.';
    });
    _scheduleYtDlpChecks();
    if (enabled && _ytDlpManagerSupported) {
      await _checkYtDlpUpdates(silent: true);
    }
  }

  Future<String> _installedYtDlpExecutable() async {
    if (!_ytDlpSupported) {
      throw StateError('yt-dlp is supported on Windows and Linux only.');
    }
    final root = await _ytDlpInstallRoot();
    final activeFile = File('$root${Platform.pathSeparator}active.json');
    if (!await activeFile.exists()) {
      throw StateError('yt-dlp is not installed. Install it from the yt-dlp manager first.');
    }
    final metadata = jsonDecode(await activeFile.readAsString()) as Map<String, dynamic>;
    final relative = metadata['executablePath'] as String?;
    if (relative == null) throw const FormatException('Missing executable path');
    final segments = relative.split('/');
    if (segments.length < 3 || segments.first != 'versions' ||
        segments.any((part) => part.isEmpty || part == '.' || part == '..')) {
      throw const FormatException('Unsafe yt-dlp executable path');
    }
    final executable = '$root${Platform.pathSeparator}${segments.join(Platform.pathSeparator)}';
    if (!await File(executable).exists()) {
      throw StateError('The active yt-dlp executable is missing.');
    }
    return executable;
  }

  Future<void> _refreshInstalledYtDlp() async {
    if (!_ytDlpManagerSupported) return;
    try {
      final version = Platform.isAndroid
          ? await _ytDlpAndroidChannel.invokeMethod<String>('version')
          : (await rust_api.ytDlpGetVersion(executablePath: await _installedYtDlpExecutable())).trim();
      if (mounted) setState(() => _ytDlpInstalledVersion = version);
    } catch (error) {
      if (mounted) {
        setState(() {
          _ytDlpInstalledVersion = null;
          _ytDlpStatus = 'Installed yt-dlp could not be verified: $error';
        });
      }
    }
  }

  Future<void> _rollbackYtDlp() async {
    if (!_ytDlpSupported || _ytDlpInstalling || _ytDlpRollingBack) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('Restore previous yt-dlp version?'),
        content: const Text(
          'The currently active yt-dlp version and the previous verified version will be swapped.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('Restore'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    setState(() {
      _ytDlpRollingBack = true;
      _ytDlpStatus = 'Validating and restoring the previous yt-dlp version...';
    });
    try {
      final root = await _ytDlpInstallRoot();
      final raw = await rust_api.ytDlpRollback(installRoot: root);
      final restored = jsonDecode(raw) as Map<String, dynamic>;
      await _refreshInstalledYtDlp();
      final version = restored['version'] as String? ?? _ytDlpInstalledVersion ?? 'unknown';
      final entry = '\${DateTime.now().toLocal().toString().substring(0, 16)} — Restored yt-dlp $version';
      final prefs = await SharedPreferences.getInstance();
      final history = [entry, ..._ytDlpHistory].take(10).toList();
      await prefs.setStringList(_ytDlpHistoryKey, history);
      if (!mounted) return;
      setState(() {
        _ytDlpHistory = history;
        _ytDlpStatus = 'Previous yt-dlp version $version restored and verified.';
      });
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('yt-dlp $version restored successfully.')),
      );
    } catch (error) {
      if (mounted) {
        setState(() => _ytDlpStatus = 'Rollback failed; the active version was not changed: $error');
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Could not restore yt-dlp: $error')),
        );
      }
    } finally {
      if (mounted) setState(() => _ytDlpRollingBack = false);
    }
  }

  Future<void> _checkYtDlpUpdates({bool silent = false}) async {
    if (!_ytDlpManagerSupported || _ytDlpChecking || _ytDlpInstalling) return;
    if (mounted) {
      setState(() {
        _ytDlpChecking = true;
        _ytDlpStatus = 'Checking the official yt-dlp release...';
      });
    }
    try {
      final String? latest;
      if (Platform.isAndroid) {
        final release = await _ytDlpAndroidChannel.invokeMapMethod<String, dynamic>('checkLatest');
        latest = release?['latestVersion'] as String?;
        final installed = release?['installedVersion'] as String?;
        if (installed != null && installed.isNotEmpty) _ytDlpInstalledVersion = installed;
      } else {
        final raw = await rust_api.ytDlpCheckLatestRelease();
        final release = jsonDecode(raw) as Map<String, dynamic>;
        latest = release['version'] as String?;
      }
      if (latest == null || latest.isEmpty) {
        throw const FormatException('The official release response has no version.');
      }
      final checkedAt = DateTime.now().toIso8601String();
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(_ytDlpLastCheckKey, checkedAt);
      if (!mounted) return;
      setState(() {
        _ytDlpLatestVersion = latest;
        _ytDlpLastCheckedAt = checkedAt;
        _ytDlpStatus = _ytDlpInstalledVersion == latest
            ? 'yt-dlp is up to date.'
            : 'A new yt-dlp version is available.';
      });
      if (!silent) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(_ytDlpInstalledVersion == latest
              ? 'yt-dlp is already up to date.'
              : 'yt-dlp $latest is available.')),
        );
      }
    } catch (error) {
      if (mounted) {
        setState(() => _ytDlpStatus = 'Update check failed: $error');
        if (!silent) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Could not check yt-dlp updates: $error')),
          );
        }
      }
    } finally {
      if (mounted) setState(() => _ytDlpChecking = false);
    }
  }

  Future<void> _installLatestYtDlp() async {
    if (!_ytDlpManagerSupported || _ytDlpInstalling || _ytDlpChecking ||
        _ytDlpLatestVersion == null) return;
    setState(() {
      _ytDlpInstalling = true;
      _ytDlpReceivedBytes = 0;
      _ytDlpTotalBytes = null;
      _ytDlpStatus = 'Downloading and verifying the official yt-dlp release...';
    });
    try {
      if (Platform.isAndroid) {
        final update = await _ytDlpAndroidChannel.invokeMapMethod<String, dynamic>('updateStable');
        await _refreshInstalledYtDlp();
        final status = update?['status'] as String? ?? 'UPDATED';
        if (mounted) setState(() => _ytDlpStatus = status == 'ALREADY_UP_TO_DATE'
            ? 'Android yt-dlp is already up to date.'
            : 'Android yt-dlp updated from the official stable release.');
      } else {
        final root = await _ytDlpInstallRoot();
        await for (final raw in rust_api.ytDlpInstallLatestRelease(installRoot: root)) {
        try {
          final event = jsonDecode(raw) as Map<String, dynamic>;
          final phase = event['phase'] as String?;
          if (phase == 'downloading') {
            if (mounted) {
              setState(() {
                _ytDlpReceivedBytes = (event['received'] as num?)?.toInt() ?? 0;
                _ytDlpTotalBytes = (event['total'] as num?)?.toInt();
                _ytDlpStatus = 'Downloading yt-dlp...';
              });
            }
          } else if (phase == 'installed') {
            if (mounted) {
              setState(() => _ytDlpStatus = 'yt-dlp ${event['version']} installed and verified.');
            }
          }
        } catch (_) {
          // Ignore non-JSON stream messages; the stream error channel reports failures.
        }
        }
      }
      await _refreshInstalledYtDlp();
      final version = _ytDlpInstalledVersion ?? _ytDlpLatestVersion ?? 'latest';
      final entry = '${DateTime.now().toLocal().toString().substring(0, 16)} — Installed yt-dlp $version';
      final prefs = await SharedPreferences.getInstance();
      final history = [entry, ..._ytDlpHistory].take(10).toList();
      await prefs.setStringList(_ytDlpHistoryKey, history);
      if (!mounted) return;
      setState(() {
        _ytDlpHistory = history;
        _ytDlpStatus = 'yt-dlp $version installed and verified.';
      });
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('yt-dlp $version installed successfully.')),
      );
    } catch (error) {
      if (mounted) {
        setState(() => _ytDlpStatus = 'Installation failed; the active version was not replaced: $error');
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('yt-dlp installation failed: $error')),
        );
      }
    } finally {
      if (mounted) setState(() => _ytDlpInstalling = false);
    }
  }

  bool get _ffmpegSupported => Platform.isWindows || Platform.isLinux;

  Future<String> _ffmpegInstallRoot() async {
    final support = await getApplicationSupportDirectory();
    return '${support.path}${Platform.pathSeparator}managed-tools${Platform.pathSeparator}ffmpeg';
  }

  Future<void> _initializeFfmpegManager() async {
    if (!_ffmpegSupported) {
      if (mounted) {
        setState(() => _ffmpegStatus =
            'FFmpeg managed installation and conversion are available on Windows and Linux only.');
      }
      return;
    }
    final prefs = await SharedPreferences.getInstance();
    if (!mounted) return;
    setState(() {
      _ffmpegAutoCheckEnabled = prefs.getBool(_ffmpegAutoCheckKey) ?? true;
      _ffmpegLastCheckedAt = prefs.getString(_ffmpegLastCheckKey);
      _ffmpegHistory = prefs.getStringList(_ffmpegHistoryKey) ?? [];
    });

    var managedInstalled = false;
    try {
      await _refreshManagedFfmpeg();
      managedInstalled = _ffmpegExecutablePath != null;
    } catch (_) {
      // Fall back to a previously selected executable if no managed version exists.
    }
    if (!managedInstalled) {
      final savedPath = prefs.getString(_ffmpegExecutableKey);
      if (savedPath != null && savedPath.isNotEmpty) {
        try {
          await _verifyFfmpegExecutable(savedPath, persist: false);
        } catch (_) {
          // The manager card displays the verification error.
        }
      } else if (mounted) {
        setState(() => _ffmpegStatus = 'FFmpeg is not installed yet. Check for an official update to install it.');
      }
    }

    if (_ffmpegAutoCheckEnabled) {
      await _checkFfmpegUpdates(silent: true);
      _scheduleFfmpegChecks();
    } else if (mounted) {
      setState(() => _ffmpegStatus = 'Automatic FFmpeg update checks are paused.');
    }
  }

  void _scheduleFfmpegChecks() {
    _ffmpegUpdateTimer?.cancel();
    if (!_ffmpegSupported || !_ffmpegAutoCheckEnabled) return;
    _ffmpegUpdateTimer = Timer.periodic(const Duration(hours: 24), (_) {
      unawaited(_checkFfmpegUpdates(silent: true));
    });
  }

  Future<void> _setFfmpegAutoCheck(bool enabled) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_ffmpegAutoCheckKey, enabled);
    if (!mounted) return;
    setState(() {
      _ffmpegAutoCheckEnabled = enabled;
      _ffmpegStatus = enabled
          ? 'Automatic FFmpeg checks enabled (every 24 hours while the app is open).'
          : 'Automatic FFmpeg update checks are paused.';
    });
    _scheduleFfmpegChecks();
    if (enabled && _ffmpegSupported) {
      await _checkFfmpegUpdates(silent: true);
    }
  }

  Future<void> _refreshManagedFfmpeg() async {
    final root = await _ffmpegInstallRoot();
    final raw = await rust_api.ffmpegGetActiveInstallation(installRoot: root);
    final active = jsonDecode(raw) as Map<String, dynamic>;
    final relative = active['executablePath'] as String?;
    final version = active['version'] as String?;
    if (relative == null || version == null || version.isEmpty) {
      throw const FormatException('The active FFmpeg installation metadata is incomplete.');
    }
    final segments = relative.split('/');
    if (segments.length < 3 || segments.first != 'versions' ||
        segments.any((part) => part.isEmpty || part == '.' || part == '..')) {
      throw const FormatException('The active FFmpeg executable path is unsafe.');
    }
    final executable = '$root${Platform.pathSeparator}${segments.join(Platform.pathSeparator)}';
    if (!await File(executable).exists()) {
      throw StateError('The active managed FFmpeg executable is missing.');
    }
    if (!mounted) return;
    setState(() {
      _ffmpegExecutablePath = executable;
      _ffmpegVersion = version;
      _ffmpegStatus = 'Managed FFmpeg $version is verified and ready.';
    });
  }

  Future<void> _checkFfmpegUpdates({bool silent = false}) async {
    if (!_ffmpegSupported || _ffmpegChecking || _ffmpegInstalling) return;
    if (mounted) {
      setState(() {
        _ffmpegChecking = true;
        _ffmpegStatus = 'Checking the official BtbN FFmpeg release and checksum...';
      });
    }
    try {
      final raw = await rust_api.ffmpegCheckLatestRelease();
      final release = jsonDecode(raw) as Map<String, dynamic>;
      final latest = release['version'] as String?;
      if (latest == null || latest.isEmpty) {
        throw const FormatException('The official FFmpeg release response has no version.');
      }
      final checkedAt = DateTime.now().toIso8601String();
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(_ffmpegLastCheckKey, checkedAt);
      if (!mounted) return;
      setState(() {
        _ffmpegLatestVersion = latest;
        _ffmpegLastCheckedAt = checkedAt;
        _ffmpegStatus = _ffmpegVersion == latest
            ? 'FFmpeg is up to date.'
            : 'A verified FFmpeg update is available.';
      });
      if (!silent) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(_ffmpegVersion == latest
              ? 'FFmpeg is already up to date.'
              : 'FFmpeg $latest is available.')),
        );
      }
    } catch (error) {
      if (mounted) {
        setState(() => _ffmpegStatus = 'FFmpeg update check failed: $error');
        if (!silent) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Could not check FFmpeg updates: $error')),
          );
        }
      }
    } finally {
      if (mounted) setState(() => _ffmpegChecking = false);
    }
  }

  Future<void> _installLatestFfmpeg() async {
    if (!_ffmpegSupported || _ffmpegInstalling || _ffmpegChecking ||
        _ffmpegLatestVersion == null || _ffmpegConverting) return;
    setState(() {
      _ffmpegInstalling = true;
      _ffmpegReceivedBytes = 0;
      _ffmpegTotalBytes = null;
      _ffmpegStatus = 'Downloading and verifying the official FFmpeg build...';
    });
    try {
      final root = await _ffmpegInstallRoot();
      await for (final raw in rust_api.ffmpegInstallLatestRelease(installRoot: root)) {
        try {
          final event = jsonDecode(raw) as Map<String, dynamic>;
          final phase = event['phase'] as String? ?? 'installing';
          if (mounted) {
            setState(() {
              _ffmpegReceivedBytes = (event['received'] as num?)?.toInt() ?? _ffmpegReceivedBytes;
              _ffmpegTotalBytes = (event['total'] as num?)?.toInt() ?? _ffmpegTotalBytes;
              _ffmpegStatus = switch (phase) {
                'downloading' => 'Downloading FFmpeg archive...',
                'verifying' => 'Verifying FFmpeg SHA-256 checksum...',
                'extracting' => 'Safely extracting FFmpeg...',
                'probing' => 'Checking FFmpeg version and MP3 encoder...',
                'installed' => 'FFmpeg installed and verified.',
                _ => 'Installing FFmpeg ($phase)...',
              };
            });
          }
        } catch (_) {
          // The bridge error channel reports stream failures.
        }
      }
      await _refreshManagedFfmpeg();
      final version = _ffmpegVersion ?? _ffmpegLatestVersion ?? 'latest';
      final entry = '${DateTime.now().toLocal().toString().substring(0, 16)} — Installed FFmpeg $version';
      final prefs = await SharedPreferences.getInstance();
      final history = [entry, ..._ffmpegHistory].take(10).toList();
      await prefs.setStringList(_ffmpegHistoryKey, history);
      if (!mounted) return;
      setState(() {
        _ffmpegHistory = history;
        _ffmpegStatus = 'FFmpeg $version installed and verified.';
      });
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('FFmpeg $version installed successfully.')),
      );
    } catch (error) {
      if (mounted) {
        setState(() => _ffmpegStatus = 'FFmpeg installation failed; the previous active version was preserved: $error');
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('FFmpeg installation failed: $error')),
        );
      }
    } finally {
      if (mounted) setState(() => _ffmpegInstalling = false);
    }
  }

  Future<void> _rollbackFfmpeg() async {
    if (!_ffmpegSupported || _ffmpegInstalling || _ffmpegConverting) return;
    try {
      if (mounted) setState(() => _ffmpegStatus = 'Validating and restoring the previous FFmpeg version...');
      final root = await _ffmpegInstallRoot();
      await rust_api.ffmpegRollback(installRoot: root);
      await _refreshManagedFfmpeg();
      if (mounted) {
        setState(() => _ffmpegStatus = 'Previous FFmpeg version restored and verified.');
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Previous FFmpeg version restored.')),
        );
      }
    } catch (error) {
      if (mounted) setState(() => _ffmpegStatus = 'FFmpeg rollback unavailable or failed: $error');
    }
  }

  Future<void> _chooseFfmpegExecutable() async {
    if (!_ffmpegSupported || _ffmpegConverting) return;
    try {
      final result = await FilePicker.pickFile(type: FileType.any);
      final path = result?.path;
      if (path == null) return;
      await _verifyFfmpegExecutable(path, persist: true);
    } catch (error) {
      if (!mounted) return;
      setState(() => _ffmpegStatus = 'Could not select FFmpeg: $error');
    }
  }

  Future<void> _verifyFfmpegExecutable(String path, {required bool persist}) async {
    if (!await File(path).exists()) {
      throw StateError('The selected FFmpeg file does not exist.');
    }
    if (mounted) setState(() => _ffmpegStatus = 'Verifying FFmpeg executable...');
    try {
      final version = (await rust_api.ffmpegGetVersion(executablePath: path)).trim();
      if (version.isEmpty) throw StateError('FFmpeg returned an empty version.');
      if (persist) {
        final prefs = await SharedPreferences.getInstance();
        await prefs.setString(_ffmpegExecutableKey, path);
      }
      if (!mounted) return;
      setState(() {
        _ffmpegExecutablePath = path;
        _ffmpegVersion = version;
        _ffmpegStatus = 'FFmpeg $version verified and ready.';
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _ffmpegExecutablePath = null;
        _ffmpegVersion = null;
        _ffmpegStatus = 'FFmpeg verification failed: $error';
      });
      rethrow;
    }
  }

  Future<void> _chooseVideoForMp3() async {
    if (!_ffmpegSupported || _ffmpegConverting) return;
    try {
      final result = await FilePicker.pickFile(type: FileType.video);
      final path = result?.path;
      if (path == null || !mounted) return;
      setState(() {
        _ffmpegInputPath = path;
        _ffmpegStatus = 'Video selected. Choose a bitrate and start conversion.';
        _ffmpegElapsedMillis = 0;
        _ffmpegTotalMillis = null;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() => _ffmpegStatus = 'Could not select video: $error');
    }
  }

  Future<void> _convertSelectedVideoToMp3() async {
    final executable = _ffmpegExecutablePath;
    final input = _ffmpegInputPath;
    if (!_ffmpegSupported || executable == null || input == null || _ffmpegConverting) {
      return;
    }
    try {
      final inputName = input.split(RegExp(r'[\\/]')).last;
      final baseName = inputName.contains('.')
          ? inputName.substring(0, inputName.lastIndexOf('.'))
          : inputName;
      final outputDirectory = await FilePicker.getDirectoryPath(
        dialogTitle: 'Choose folder for converted MP3',
      );
      if (outputDirectory == null || !mounted) return;
      final separator = Platform.pathSeparator;
      final output = outputDirectory.endsWith(separator)
          ? '$outputDirectory$baseName.mp3'
          : '$outputDirectory$separator$baseName.mp3';
      if (!output.toLowerCase().endsWith('.mp3')) {
        throw const FormatException('Choose an output filename ending in .mp3.');
      }
      if (await File(output).exists()) {
        throw StateError('The selected MP3 already exists. Choose a new filename.');
      }

      final taskId = 'ffmpeg-' + DateTime.now().microsecondsSinceEpoch.toString();
      setState(() {
        _ffmpegConverting = true;
        _ffmpegPaused = false;
        _ffmpegCancelRequested = false;
        _ffmpegTaskId = taskId;
        _ffmpegElapsedMillis = 0;
        _ffmpegTotalMillis = null;
        _ffmpegStatus = 'Starting MP3 conversion...';
      });

      await for (final raw in rust_api.ffmpegConvertToMp3Controlled(
        taskId: taskId,
        executablePath: executable,
        inputPath: input,
        outputPath: output,
        bitrate: _ffmpegBitrate,
      )) {
        try {
          final event = jsonDecode(raw) as Map<String, dynamic>;
          if (event['phase'] == 'converting' && mounted) {
            setState(() {
              _ffmpegElapsedMillis =
                  (event['elapsedMillis'] as num?)?.toInt() ?? _ffmpegElapsedMillis;
              _ffmpegTotalMillis =
                  (event['totalMillis'] as num?)?.toInt() ?? _ffmpegTotalMillis;
              _ffmpegStatus = 'Converting video to MP3...';
            });
          }
        } catch (_) {
          // Stream errors are delivered through the bridge error channel.
        }
      }

      if (!mounted) return;
      setState(() => _ffmpegStatus = _ffmpegCancelRequested
          ? 'Conversion cancelled. The incomplete MP3 was removed.'
          : 'MP3 conversion completed successfully: $output');
      if (!_ffmpegCancelRequested) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('MP3 conversion completed.')),
        );
      }
    } catch (error) {
      if (!mounted) return;
      setState(() => _ffmpegStatus = _ffmpegCancelRequested
          ? 'Conversion cancelled.'
          : 'MP3 conversion failed: $error');
    } finally {
      if (mounted) {
        setState(() {
          _ffmpegConverting = false;
          _ffmpegPaused = false;
          _ffmpegTaskId = null;
        });
      }
    }
  }

  Future<void> _pauseFfmpegConversion() async {
    final taskId = _ffmpegTaskId;
    if (taskId == null || !_ffmpegConverting || _ffmpegPaused) return;
    try {
      await rust_api.pauseDownload(taskId: taskId);
      if (mounted) setState(() {
        _ffmpegPaused = true;
        _ffmpegStatus = 'MP3 conversion paused.';
      });
    } catch (error) {
      if (mounted) setState(() => _ffmpegStatus = 'Could not pause conversion: $error');
    }
  }

  Future<void> _resumeFfmpegConversion() async {
    final taskId = _ffmpegTaskId;
    if (taskId == null || !_ffmpegConverting || !_ffmpegPaused) return;
    try {
      await rust_api.resumeDownload(taskId: taskId);
      if (mounted) setState(() {
        _ffmpegPaused = false;
        _ffmpegStatus = 'Resuming MP3 conversion...';
      });
    } catch (error) {
      if (mounted) setState(() => _ffmpegStatus = 'Could not resume conversion: $error');
    }
  }

  Future<void> _cancelFfmpegConversion() async {
    final taskId = _ffmpegTaskId;
    if (taskId == null || !_ffmpegConverting) return;
    try {
      setState(() {
        _ffmpegCancelRequested = true;
        _ffmpegStatus = 'Cancelling conversion and cleaning temporary output...';
      });
      await rust_api.cancelDownload(taskId: taskId);
    } catch (error) {
      if (mounted) setState(() => _ffmpegStatus = 'Could not cancel conversion: $error');
    }
  }

  Widget _buildFfmpegManagerCard() {
    final supported = _ffmpegSupported;
    final progress = _ffmpegTotalMillis != null && _ffmpegTotalMillis! > 0
        ? (_ffmpegElapsedMillis / _ffmpegTotalMillis!).clamp(0.0, 1.0).toDouble()
        : null;
    String timeLabel(int millis) {
      final seconds = millis ~/ 1000;
      final minutes = seconds ~/ 60;
      return minutes.toString().padLeft(2, '0') + ':' +
          (seconds % 60).toString().padLeft(2, '0');
    }

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                const Icon(Icons.graphic_eq_rounded, color: Color(0xFF60A5FA), size: 25),
                const SizedBox(width: 10),
                const Expanded(
                  child: Text('FFmpeg manager',
                      style: TextStyle(fontSize: 17, fontWeight: FontWeight.w800)),
                ),
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 5),
                  decoration: BoxDecoration(
                    color: supported
                        ? const Color(0xFF16A34A).withValues(alpha: 0.14)
                        : Colors.orange.withValues(alpha: 0.14),
                    borderRadius: BorderRadius.circular(20),
                  ),
                  child: Text(supported ? 'DESKTOP' : 'NOT AVAILABLE',
                      style: TextStyle(
                        color: supported ? const Color(0xFF4ADE80) : Colors.orangeAccent,
                        fontSize: 10,
                        fontWeight: FontWeight.w800,
                      )),
                ),
              ],
            ),
            const SizedBox(height: 12),
            Text('Installed: ' + (_ffmpegVersion ?? 'Not installed')),
            Text('Latest official: ${_ffmpegLatestVersion ?? 'Not checked'}'),
            Text(
              'Last check: ${_ffmpegLastCheckedAt == null ? 'Not checked yet' : (DateTime.tryParse(_ffmpegLastCheckedAt!)?.toLocal().toString().substring(0, 16) ?? 'Unknown')}',
              style: const TextStyle(color: Color(0xFF9A9AAF), fontSize: 12),
            ),
            if (_ffmpegExecutablePath != null) ...[
              const SizedBox(height: 4),
              Text(_ffmpegExecutablePath!,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(color: Color(0xFF9A9AAF), fontSize: 11)),
            ],
            const SizedBox(height: 10),
            Text(_ffmpegStatus, style: const TextStyle(color: Color(0xFFBDBDCE))),
            if (_ffmpegInstalling) ...[
              const SizedBox(height: 10),
              LinearProgressIndicator(
                value: _ffmpegTotalBytes != null && _ffmpegTotalBytes! > 0
                    ? (_ffmpegReceivedBytes / _ffmpegTotalBytes!).clamp(0.0, 1.0).toDouble()
                    : null,
              ),
              const SizedBox(height: 5),
              Text(
                _ffmpegTotalBytes == null
                    ? _formatYtDlpBytes(_ffmpegReceivedBytes)
                    : '${_formatYtDlpBytes(_ffmpegReceivedBytes)} / ${_formatYtDlpBytes(_ffmpegTotalBytes!)}',
                style: const TextStyle(fontSize: 12, color: Color(0xFF9A9AAF)),
              ),
            ],
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                OutlinedButton.icon(
                  onPressed: supported && !_ffmpegConverting ? _chooseFfmpegExecutable : null,
                  icon: const Icon(Icons.folder_open_rounded),
                  label: Text(_ffmpegVersion == null ? 'Choose FFmpeg' : 'Change FFmpeg'),
                ),
                OutlinedButton.icon(
                  onPressed: supported && !_ffmpegConverting && !_ffmpegChecking && !_ffmpegInstalling
                      ? () => _checkFfmpegUpdates()
                      : null,
                  icon: _ffmpegChecking
                      ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2))
                      : const Icon(Icons.refresh_rounded),
                  label: const Text('Check updates'),
                ),
                FilledButton.icon(
                  onPressed: supported && !_ffmpegConverting && !_ffmpegChecking &&
                          !_ffmpegInstalling && _ffmpegLatestVersion != null &&
                          _ffmpegLatestVersion != _ffmpegVersion
                      ? _installLatestFfmpeg
                      : null,
                  icon: const Icon(Icons.system_update_alt_rounded),
                  label: Text(_ffmpegInstalling ? 'Updating...' : 'Install / Update'),
                ),
                OutlinedButton.icon(
                  onPressed: supported && !_ffmpegConverting && !_ffmpegInstalling
                      ? _rollbackFfmpeg
                      : null,
                  icon: const Icon(Icons.history_rounded),
                  label: const Text('Rollback'),
                ),
                OutlinedButton.icon(
                  onPressed: supported && !_ffmpegConverting && _ffmpegExecutablePath != null
                      ? () => _verifyFfmpegExecutable(_ffmpegExecutablePath!, persist: true)
                      : null,
                  icon: const Icon(Icons.verified_outlined),
                  label: const Text('Verify'),
                ),
              ],
            ),
            const Divider(height: 24),
            const Text('Video to MP3', style: TextStyle(fontWeight: FontWeight.w800)),
            const SizedBox(height: 6),
            OutlinedButton.icon(
              onPressed: supported && !_ffmpegConverting ? _chooseVideoForMp3 : null,
              icon: const Icon(Icons.video_file_outlined),
              label: Text(_ffmpegInputPath == null
                  ? 'Choose video file'
                  : _ffmpegInputPath!.split(RegExp(r'[\\/]')).last),
            ),
            const SizedBox(height: 10),
            DropdownButtonFormField<String>(
              value: _ffmpegBitrate,
              decoration: const InputDecoration(
                labelText: 'MP3 quality',
                border: OutlineInputBorder(),
                isDense: true,
              ),
              items: const ['128k', '160k', '192k', '256k', '320k']
                  .map((rate) => DropdownMenuItem(value: rate, child: Text(rate)))
                  .toList(),
              onChanged: supported && !_ffmpegConverting
                  ? (value) {
                      if (value != null) setState(() => _ffmpegBitrate = value);
                    }
                  : null,
            ),
            if (_ffmpegConverting) ...[
              const SizedBox(height: 12),
              LinearProgressIndicator(value: progress),
              const SizedBox(height: 5),
              Text(
                _ffmpegTotalMillis == null
                    ? timeLabel(_ffmpegElapsedMillis)
                    : timeLabel(_ffmpegElapsedMillis) + ' / ' + timeLabel(_ffmpegTotalMillis!),
                style: const TextStyle(fontSize: 12, color: Color(0xFF9A9AAF)),
              ),
            ],
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                FilledButton.icon(
                  onPressed: supported &&
                          !_ffmpegConverting &&
                          _ffmpegExecutablePath != null &&
                          _ffmpegInputPath != null
                      ? _convertSelectedVideoToMp3
                      : null,
                  icon: const Icon(Icons.music_note_rounded),
                  label: const Text('Convert to MP3'),
                ),
                if (_ffmpegConverting)
                  OutlinedButton.icon(
                    onPressed: _ffmpegPaused ? _resumeFfmpegConversion : _pauseFfmpegConversion,
                    icon: Icon(_ffmpegPaused ? Icons.play_arrow_rounded : Icons.pause_rounded),
                    label: Text(_ffmpegPaused ? 'Resume' : 'Pause'),
                  ),
                if (_ffmpegConverting)
                  OutlinedButton.icon(
                    onPressed: _cancelFfmpegConversion,
                    icon: const Icon(Icons.close_rounded),
                    label: const Text('Cancel'),
                  ),
              ],
            ),
            if (supported) ...[
              SwitchListTile.adaptive(
                contentPadding: EdgeInsets.zero,
                title: const Text('Automatic update checks'),
                subtitle: const Text('Check the official release every 24 hours while Easy Download is open.'),
                value: _ffmpegAutoCheckEnabled,
                onChanged: !_ffmpegInstalling && !_ffmpegChecking ? _setFfmpegAutoCheck : null,
              ),
              if (_ffmpegHistory.isNotEmpty) ...[
                const SizedBox(height: 4),
                const Text('Recent FFmpeg updates', style: TextStyle(fontWeight: FontWeight.w700)),
                const SizedBox(height: 4),
                ..._ffmpegHistory.take(3).map((entry) => Padding(
                  padding: const EdgeInsets.only(bottom: 3),
                  child: Text(entry, style: const TextStyle(fontSize: 11, color: Color(0xFF9A9AAF))),
                )),
              ],
            ],
            if (!supported) ...[
              const SizedBox(height: 8),
              const Text(
                'FFmpeg managed installation and conversion are disabled on Android until a compatible, verified runtime is provided.',
                style: TextStyle(fontSize: 12, color: Color(0xFFFFB86B)),
              ),
            ],
          ],
        ),
      ),
    );
  }

  String _formatYtDlpBytes(int bytes) {
    if (bytes < 1024) return '$bytes B';
    if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(1)} KB';
    return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
  }

  Widget _buildYtDlpManagerCard() {
    final supported = _ytDlpManagerSupported;
    final hasUpdate = _ytDlpLatestVersion != null &&
        _ytDlpLatestVersion != _ytDlpInstalledVersion;
    final progress = _ytDlpTotalBytes != null && _ytDlpTotalBytes! > 0
        ? (_ytDlpReceivedBytes / _ytDlpTotalBytes!).clamp(0.0, 1.0).toDouble()
        : null;
    final lastCheck = _ytDlpLastCheckedAt == null
        ? 'Not checked yet'
        : DateTime.tryParse(_ytDlpLastCheckedAt!)?.toLocal().toString().substring(0, 16) ?? 'Unknown';

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                const Icon(Icons.video_library_outlined, color: EasyDownloadPalette.accent, size: 25),
                const SizedBox(width: 10),
                const Expanded(
                  child: Text('yt-dlp manager',
                      style: TextStyle(fontSize: 17, fontWeight: FontWeight.w800)),
                ),
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 5),
                  decoration: BoxDecoration(
                    color: supported
                        ? const Color(0xFF16A34A).withValues(alpha: 0.14)
                        : Colors.orange.withValues(alpha: 0.14),
                    borderRadius: BorderRadius.circular(20),
                  ),
                  child: Text(supported ? 'DESKTOP' : 'NOT AVAILABLE',
                      style: TextStyle(
                        color: supported ? const Color(0xFF4ADE80) : Colors.orangeAccent,
                        fontSize: 10,
                        fontWeight: FontWeight.w800,
                      )),
                ),
              ],
            ),
            const SizedBox(height: 12),
            Text('Installed: ${_ytDlpInstalledVersion ?? 'Not installed'}'),
            const SizedBox(height: 4),
            Text('Latest official: ${_ytDlpLatestVersion ?? 'Not checked'}'),
            const SizedBox(height: 4),
            Text('Last check: $lastCheck',
                style: const TextStyle(color: Color(0xFF9A9AAF), fontSize: 12)),
            const SizedBox(height: 10),
            Text(_ytDlpStatus, style: const TextStyle(color: Color(0xFFBDBDCE))),
            if (_ytDlpInstalling) ...[
              const SizedBox(height: 10),
              LinearProgressIndicator(value: progress),
              const SizedBox(height: 5),
              Text(
                _ytDlpTotalBytes == null
                    ? _formatYtDlpBytes(_ytDlpReceivedBytes)
                    : '${_formatYtDlpBytes(_ytDlpReceivedBytes)} / ${_formatYtDlpBytes(_ytDlpTotalBytes!)}',
                style: const TextStyle(fontSize: 12, color: Color(0xFF9A9AAF)),
              ),
            ],
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                OutlinedButton.icon(
                  onPressed: supported && !_ytDlpChecking && !_ytDlpInstalling
                      ? () => _checkYtDlpUpdates()
                      : null,
                  icon: _ytDlpChecking
                      ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2))
                      : const Icon(Icons.refresh_rounded),
                  label: const Text('Check updates'),
                ),
                FilledButton.icon(
                  onPressed: supported && hasUpdate && !_ytDlpChecking && !_ytDlpInstalling
                      ? _installLatestYtDlp
                      : null,
                  icon: _ytDlpInstalling
                      ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2))
                      : const Icon(Icons.system_update_alt_rounded),
                  label: Text(_ytDlpInstalledVersion == null ? 'Install yt-dlp' : 'Update now'),
                ),
                OutlinedButton.icon(
                  onPressed: supported && _ytDlpInstalledVersion != null &&
                          !_ytDlpInstalling && !_ytDlpRollingBack
                      ? _rollbackYtDlp
                      : null,
                  icon: _ytDlpRollingBack
                      ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2))
                      : const Icon(Icons.history_rounded),
                  label: const Text('Rollback'),
                ),
              ],
            ),
            const Divider(height: 22),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              dense: true,
              title: const Text('Automatic update checks'),
              subtitle: const Text('Every 24 hours while the app is open'),
              value: _ytDlpAutoCheckEnabled,
              onChanged: supported && !_ytDlpInstalling ? _setYtDlpAutoCheck : null,
            ),
            if (_ytDlpHistory.isNotEmpty) ...[
              const Divider(height: 18),
              const Text('Update history', style: TextStyle(fontWeight: FontWeight.w700)),
              const SizedBox(height: 6),
              for (final item in _ytDlpHistory.take(3))
                Padding(
                  padding: const EdgeInsets.only(bottom: 4),
                  child: Text(item, style: const TextStyle(fontSize: 12, color: Color(0xFF9A9AAF))),
                ),
            ],
            if (!supported) ...[
              const SizedBox(height: 8),
              const Text(
                'Android support is intentionally disabled until an official compatible yt-dlp distribution is available.',
                style: TextStyle(fontSize: 12, color: Color(0xFFFFB86B)),
              ),
            ],
          ],
        ),
      ),
    );
  }

  Future<void> _loadHistory() async {
    final prefs = await SharedPreferences.getInstance();
    final store = await _getHistoryStore();
    final storedEntries = await store.load();
    final legacyRaw = prefs.getString(_historyKey);
    if (!mounted) return;

    setState(() {
      _downloadDirectoryPath = prefs.getString(_downloadDirectoryKey);
      _androidDownloadTreeUri = prefs.getString(_androidDirectoryKey);
    });

    try {
      List<dynamic>? entries = storedEntries;
      if (entries == null && legacyRaw != null) {
        final decoded = jsonDecode(legacyRaw);
        if (decoded is List) entries = decoded;
      }
      if (entries == null) return;

      final restored = entries
          .whereType<Map<String, dynamic>>()
          .map(_LinkDraft.fromJson)
          .whereType<_LinkDraft>()
          .toList();
      setState(() {
        _links
          ..clear()
          ..addAll(restored);
        // A running task cannot survive an app restart. Its partial file and
        // byte count are retained so the user can safely resume it.
        for (final link in _links) {
          link.isDownloading = false;
          link.isPaused = false;
          link.taskId = null;
          link.speedBytesPerSecond = 0;
        }
      });

      // Migrate the old SharedPreferences snapshot to the crash-resilient
      // file store, then persist the normalized recovered state.
      await _saveHistory();
    } catch (_) {
      // Keep the app usable if local history is invalid or unreadable.
    }
  }

  Future<void> _saveHistory() async {
    final store = await _getHistoryStore();
    await store.save(_links.map((item) => item.toJson()).toList());
  }

  Future<void> _exportDownloadList() async {
    if (_links.isEmpty) return;
    final lines = _links.map((link) {
      final status = link.isDownloading
          ? (link.isPaused ? 'Paused' : 'Downloading')
          : link.isFailed
              ? 'Failed'
              : link.savedPath != null
                  ? 'Completed'
                  : link.isCancelled
                      ? 'Cancelled'
                      : 'Queued';
      return '${link.fileName}\nURL: ${link.url}\nStatus: $status'
          '${link.savedPath == null ? '' : '\nSaved to: ${link.savedPath}'}';
    }).join('\n\n');
    await Clipboard.setData(ClipboardData(text: lines));
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('Download list copied to clipboard.')),
    );
  }

  Future<void> _pasteLinksFromClipboard() async {
    final data = await Clipboard.getData('text/plain');
    final raw = data?.text ?? '';
    final candidates = parseHttpLinks(raw);
    var added = 0;
    for (final uri in candidates) {
      if (_links.any((link) => link.url == uri)) continue;
      final segment = uri.pathSegments.isEmpty ? '' : uri.pathSegments.last;
      _links.add(_LinkDraft(url: uri, fileName: segment.isEmpty ? uri.host : Uri.decodeComponent(segment)));
      added++;
    }
    if (!mounted) return;
    setState(() {});
    await _saveHistory();
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(
      content: Text(added == 0 ? 'No new valid links found.' : 'Added $added link(s) from clipboard.'),
    ));
  }

  Future<void> _copyDownloadUrl(_LinkDraft link) async {
    await Clipboard.setData(ClipboardData(text: link.url.toString()));
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('Download URL copied.')),
    );
  }

  Future<void> _copySavedPath(_LinkDraft link) async {
    final path = link.savedPath;
    if (path == null) return;
    await Clipboard.setData(ClipboardData(text: path));
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('Saved file path copied.')),
    );
  }

  Future<void> _copySelectedLinks() async {
    if (_selectedLinks.isEmpty) return;
    final urls = _selectedLinks.map((link) => link.url.toString()).join('\n');
    await Clipboard.setData(ClipboardData(text: urls));
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text('Copied ${_selectedLinks.length} selected URL(s).')),
    );
  }

  Future<void> _removeSelectedLinks() async {
    final removable = _selectedLinks.where((link) => !link.isDownloading).toList();
    if (removable.isEmpty) return;
    setState(() {
      _links.removeWhere((link) => removable.contains(link));
      _selectedLinks.clear();
      _selectionMode = false;
    });
    await _saveHistory();
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text('Removed ${removable.length} selected item(s).')),
    );
  }

  Future<void> _removeLink(_LinkDraft link) async {
    if (link.isDownloading) return;
    setState(() => _links.remove(link));
    await _saveHistory();
  }

  Future<void> _renameDownload(_LinkDraft link) async {
    if (link.isDownloading || link.savedPath != null) return;
    final controller = TextEditingController(text: link.fileName);
    final newName = await showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('Rename download'),
        content: TextField(
          controller: controller,
          autofocus: true,
          textInputAction: TextInputAction.done,
          onSubmitted: (value) => Navigator.of(dialogContext).pop(value.trim()),
          decoration: const InputDecoration(
            labelText: 'File name',
            hintText: 'Enter a file name',
            border: OutlineInputBorder(),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(controller.text.trim()),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    controller.dispose();
    if (!mounted || newName == null) return;
    final cleaned = sanitizeDownloadFileName(newName);
    if (cleaned.isEmpty || cleaned == '.' || cleaned == '..') {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Enter a valid file name.')),
      );
      return;
    }
    final duplicate = _links.any((item) =>
        !identical(item, link) && item.fileName.trim().toLowerCase() == cleaned.toLowerCase());
    if (duplicate) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('A download with this file name already exists.')),
      );
      return;
    }
    setState(() => link.fileName = cleaned);
    await _saveHistory();
  }

  Future<void> _showDownloadDetails(_LinkDraft link) async {
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(link.fileName),
        content: SizedBox(
          width: 440,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                const Text('Download URL', style: TextStyle(fontWeight: FontWeight.w700)),
                const SizedBox(height: 4),
                SelectableText(link.url.toString()),
                const SizedBox(height: 12),
                const Text('File type', style: TextStyle(fontWeight: FontWeight.w700)),
                Text(_fileType(link.fileName)),
                const SizedBox(height: 12),
                const Text('Status', style: TextStyle(fontWeight: FontWeight.w700)),
                Text(link.isDownloading
                    ? (link.isPaused ? 'Paused' : 'Downloading')
                    : link.isFailed
                        ? 'Failed'
                        : link.savedPath != null
                            ? 'Completed'
                            : link.isCancelled
                                ? 'Cancelled'
                                : 'Queued'),
                if (link.totalBytes != null) ...[
                  const SizedBox(height: 12),
                  const Text('File size', style: TextStyle(fontWeight: FontWeight.w700)),
                  Text(_formatBytes(link.totalBytes!)),
                ],
                if (link.savedPath != null) ...[
                  const SizedBox(height: 12),
                  const Text('Saved location', style: TextStyle(fontWeight: FontWeight.w700)),
                  SelectableText(link.savedPath!),
                ],
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: const Text('Close'),
          ),
        ],
      ),
    );
  }

  Future<void> _clearAllHistory() async {
    final removable = _links.where((link) => !link.isDownloading).toList();
    if (removable.isEmpty) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('Clear download history?'),
        content: Text(
          'Remove ${removable.length} inactive item(s) from the list? '
          'Active downloads will continue.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('Clear history'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    setState(() => _links.removeWhere((link) => !link.isDownloading));
    await _saveHistory();
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text('Removed ${removable.length} inactive item(s).')),
    );
  }

  Future<void> _clearQueued() async {
    final queued = _links.where((link) =>
        !link.isDownloading && link.savedPath == null && !link.isFailed).toList();
    if (queued.isEmpty) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('Clear queued downloads?'),
        content: Text('Remove ${queued.length} queued item(s) from the list?'),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('Clear queued'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    setState(() => _links.removeWhere((link) =>
        !link.isDownloading && link.savedPath == null && !link.isFailed));
    await _saveHistory();
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text('Removed ${queued.length} queued item(s).')),
    );
  }

  Future<void> _clearFailed() async {
    final failed = _links.where((link) => link.isFailed && !link.isDownloading).toList();
    if (failed.isEmpty) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('Clear failed downloads?'),
        content: Text('Remove ${failed.length} failed item(s) from the list?'),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('Clear failed'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    setState(() => _links.removeWhere((link) => link.isFailed && !link.isDownloading));
    await _saveHistory();
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text('Removed ${failed.length} failed item(s).')),
    );
  }

  Future<void> _clearCompleted() async {
    final completed = _links.where((link) => link.savedPath != null).toList();
    if (completed.isEmpty) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('Clear completed downloads?'),
        content: Text(
          'Remove ${completed.length} completed item(s) from the list? '
          'The downloaded files will remain on your device.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('Clear history'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    setState(() => _links.removeWhere((link) => link.savedPath != null));
    await _saveHistory();
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text('Removed ${completed.length} completed item(s) from history.')),
    );
  }

  Future<void> _enqueueDownload(_LinkDraft link) async {
    if (link.isDownloading || link.savedPath != null || link.isFailed && link.pendingPath == null) return;
    if (mounted) setState(() {
      link.isFailed = false;
      link.isCancelled = false;
    });
    await _saveHistory();
    _pumpQueue();
  }

  void _pumpQueue() {
    if (!mounted) return;
    var active = _links.where((link) => link.isDownloading && !link.isPaused).length;
    while (active < _maxConcurrentDownloads) {
      final next = _links.cast<_LinkDraft?>().firstWhere(
        (link) => link != null &&
            !link.isDownloading &&
            link.savedPath == null &&
            !link.isFailed,
        orElse: () => null,
      );
      if (next == null) break;
      // _download marks the item active before its first asynchronous wait.
      unawaited(_download(next));
      active++;
    }
  }

  int _queuePosition(_LinkDraft link) {
    final queued = _links.where((item) =>
        !item.isDownloading && item.savedPath == null && !item.isFailed).toList();
    final index = queued.indexOf(link);
    return index < 0 ? 0 : index + 1;
  }

  Future<void> _moveQueuedDownload(_LinkDraft link, int direction) async {
    if (link.isDownloading || link.savedPath != null || link.isFailed) return;
    final queued = _links.where((item) =>
        !item.isDownloading && item.savedPath == null && !item.isFailed).toList();
    final current = queued.indexOf(link);
    final target = current + direction;
    if (current < 0 || target < 0 || target >= queued.length) return;

    final firstIndex = _links.indexOf(queued[current]);
    final secondIndex = _links.indexOf(queued[target]);
    setState(() {
      final item = _links[firstIndex];
      _links[firstIndex] = _links[secondIndex];
      _links[secondIndex] = item;
    });
    await _saveHistory();
  }

  Future<void> _download(_LinkDraft link) async {
    if (link.isDownloading || link.savedPath != null) return;
    final taskId = 'download-${DateTime.now().microsecondsSinceEpoch}';
    link.taskId = taskId;
    final isResuming = link.pendingPath != null;
    setState(() {
      link.isDownloading = true;
      link.isPaused = false;
      link.isCancelled = false;
      link.isFailed = false;
      if (!isResuming) {
        link.receivedBytes = 0;
        link.totalBytes = null;
      }
      link.speedBytesPerSecond = 0;
      link.progressPercent = null;
      link.progressEtaSeconds = null;
    });
    var previousBytes = link.receivedBytes;
    var previousTime = DateTime.now();
    try {
      final directory = !Platform.isAndroid && _downloadDirectoryPath != null
          ? Directory(_downloadDirectoryPath!)
          : await getApplicationDocumentsDirectory();
      if (!await directory.exists()) {
        throw StateError('The selected download folder no longer exists. Choose another folder in Settings.');
      }
      String destination;
      String candidateName;
      if (link.pendingPath != null) {
        destination = link.pendingPath!;
        candidateName = destination.split(Platform.pathSeparator).last;
        if (await File(destination).exists()) {
          var completedLocation = destination;
          if (Platform.isAndroid && _androidDownloadTreeUri != null) {
            completedLocation = await _copyToAndroidDownloadDirectory(destination, candidateName);
            try { await File(destination).delete(); } catch (_) {}
          }
          if (!mounted) return;
          setState(() {
            link.savedPath = completedLocation;
            link.pendingPath = null;
            link.isDownloading = false;
            link.taskId = null;
          });
          await _saveHistory();
          return;
        }
      } else {
        final finalName = sanitizeDownloadFileName(
          link.fileName,
          fallback: 'download-${DateTime.now().millisecondsSinceEpoch}',
        );
        destination = directory.path + Platform.pathSeparator + finalName;
        candidateName = finalName;
        var suffix = 1;
        while (await File(destination).exists()) {
          final dot = finalName.lastIndexOf('.');
          final hasExtension = dot > 0;
          final baseName = hasExtension ? finalName.substring(0, dot) : finalName;
          final extension = hasExtension ? finalName.substring(dot) : '';
          candidateName = '$baseName ($suffix)$extension';
          destination = directory.path + Platform.pathSeparator + candidateName;
          suffix++;
        }
        link.pendingPath = destination;
        await _saveHistory();
      }

      var transferUrl = link.url.toString();
      var transferHeadersJson = '{}';
      if (link.ytDlpFormatId != null && Platform.isAndroid) {
        final actualOutputPath = await _ytDlpAndroidChannel.invokeMethod<String>(
          'downloadFormat',
          {
            'sourceUrl': link.url.toString(),
            'formatId': link.ytDlpFormatId!,
            'outputPath': destination,
            'taskId': taskId,
          },
        );
        if (actualOutputPath == null || actualOutputPath.isEmpty) {
          throw StateError('Android yt-dlp did not return the actual output path.');
        }
        destination = actualOutputPath;
        candidateName = destination.split(Platform.pathSeparator).last;
        final outputFile = File(destination);
        if (!await outputFile.exists()) {
          throw StateError('The selected media download completed without creating its output file.');
        }
        link.receivedBytes = await outputFile.length();
        if (link.receivedBytes <= 0) {
          throw StateError('The selected media download produced an empty file.');
        }
      } else {
        if (link.ytDlpFormatId != null) {
          final executable = await _installedYtDlpExecutable();
          final resolvedRequest = jsonDecode(await rust_api.ytDlpResolveFormatRequest(
            executablePath: executable,
            sourceUrl: link.url.toString(),
            formatId: link.ytDlpFormatId!,
          )) as Map<String, dynamic>;
          transferUrl = resolvedRequest['url'] as String;
          final headers = resolvedRequest['headers'];
          if (headers is Map<String, dynamic>) {
            transferHeadersJson = jsonEncode(headers);
          }
        }

        await for (final event in rust_api.downloadFileResumableControlledWithHeaders(
          taskId: taskId,
          inputUrl: transferUrl,
          headersJson: transferHeadersJson,
          destinationPath: destination,
        )) {
          final progress = jsonDecode(event) as Map<String, dynamic>;
          final received = (progress['received'] as num).toInt();
          final total = (progress['total'] as num?)?.toInt();
          final now = DateTime.now();
          final elapsed = now.difference(previousTime).inMilliseconds / 1000;
          final speed = elapsed > 0 ? (received - previousBytes) / elapsed : 0.0;
          previousBytes = received;
          previousTime = now;
          if (!mounted) return;
          setState(() {
            link.receivedBytes = received;
            link.totalBytes = total;
            link.speedBytesPerSecond = speed < 0 ? 0 : speed;
          });
        }
      }
      var completedLocation = destination;
      if (Platform.isAndroid && _androidDownloadTreeUri != null) {
        completedLocation = await _copyToAndroidDownloadDirectory(destination, candidateName);
        try { await File(destination).delete(); } catch (_) {}
      }
      if (!mounted) return;
      setState(() {
        link.savedPath = completedLocation;
        link.pendingPath = null;
        link.isDownloading = false;
        link.isPaused = false;
        link.taskId = null;
      });
      await _saveHistory();
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Download saved: $candidateName')),
      );
    } catch (_) {
      if (!mounted) return;
      setState(() {
        link.isDownloading = false;
        link.isPaused = false;
        link.taskId = null;
        link.isFailed = !link.isCancelled;
      });
      await _saveHistory();
      if (!link.isCancelled) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Download interrupted. Retry to continue from the saved partial file.')),
        );
      }
    } finally {
      _pumpQueue();
    }
  }


  Future<void> _resumeAllPaused() async {
    final paused = _links.where((link) =>
        link.isDownloading && link.isPaused && link.taskId != null).toList();
    for (final link in paused) {
      if (!mounted) return;
      if (_links.where((item) => item.isDownloading && !item.isPaused).length >=
          _maxConcurrentDownloads) break;
      await _resumeDownload(link);
    }
    _pumpQueue();
  }

  Future<void> _pauseAllActive() async {
    final active = _links.where((link) =>
        link.isDownloading && !link.isPaused && link.taskId != null).toList();
    for (final link in active) {
      if (!mounted) return;
      await _pauseDownload(link);
    }
    _pumpQueue();
  }

  Future<void> _downloadAllQueued() async {
    _pumpQueue();
  }

  Future<void> _pauseDownload(_LinkDraft link) async {
    final taskId = link.taskId;
    if (taskId == null || !link.isDownloading || link.isPaused) return;
    if (Platform.isAndroid && link.ytDlpFormatId != null) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Pause is not supported for Android yt-dlp downloads yet.')),
      );
      return;
    }
    try {
      await rust_api.pauseDownload(taskId: taskId);
      if (mounted) setState(() => link.isPaused = true);
      _pumpQueue();
    } catch (_) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Could not pause this download.')),
        );
      }
    }
  }

  Future<void> _resumeDownload(_LinkDraft link) async {
    final taskId = link.taskId;
    if (taskId == null || !link.isDownloading || !link.isPaused) return;
    if (Platform.isAndroid && link.ytDlpFormatId != null) return;
    if (_links.where((item) => item.isDownloading && !item.isPaused).length >=
        _maxConcurrentDownloads) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('All download slots are busy. Try resume again when a slot is free.')),
        );
      }
      return;
    }
    try {
      await rust_api.resumeDownload(taskId: taskId);
      if (mounted) setState(() => link.isPaused = false);
    } catch (_) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Could not resume this download.')),
        );
      }
    }
  }

  Future<void> _cancelDownload(_LinkDraft link) async {
    final taskId = link.taskId;
    if (taskId == null || !link.isDownloading) return;

    // Mark cancellation before stopping the native process. yt-dlp may report
    // its termination to _download before the cancel method call returns.
    if (mounted) setState(() => link.isCancelled = true);
    try {
      if (Platform.isAndroid && link.ytDlpFormatId != null) {
        final cancelled = await _ytDlpAndroidChannel.invokeMethod<bool>(
          'cancelDownload',
          {'taskId': taskId},
        );
        if (cancelled != true) {
          throw StateError('The Android yt-dlp process could not be stopped.');
        }
      } else {
        await rust_api.cancelDownload(taskId: taskId);
      }
      if (mounted) {
        setState(() {
          _links.remove(link);
          link.isDownloading = false;
          link.isPaused = false;
          link.taskId = null;
        });
        await _saveHistory();
      }
    } catch (_) {
      // A failed cancellation must not leave the item marked as cancelled.
      if (mounted) {
        setState(() => link.isCancelled = false);
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Could not cancel this download.')),
        );
      }
    }
  }

  String _formatBytes(int bytes) {
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    var value = bytes.toDouble();
    var unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024;
      unit++;
    }
    return unit == 0 ? '${value.toStringAsFixed(0)} ${units[unit]}' : '${value.toStringAsFixed(1)} ${units[unit]}';
  }

  String _formatRemaining(int seconds) {
    if (seconds < 60) return '${seconds}s left';
    final minutes = seconds ~/ 60;
    final remainingSeconds = seconds % 60;
    if (minutes < 60) {
      return remainingSeconds == 0
          ? '${minutes}m left'
          : '${minutes}m ${remainingSeconds}s left';
    }
    final hours = minutes ~/ 60;
    final remainingMinutes = minutes % 60;
    return remainingMinutes == 0
        ? '${hours}h left'
        : '${hours}h ${remainingMinutes}m left';
  }

  Future<void> _addVideoLink({String? initialUrl}) async {
    if (!_ytDlpManagerSupported) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('yt-dlp video downloads are not available on this platform.')),
      );
      return;
    }
    if (_ytDlpInstalledVersion == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Install and verify yt-dlp in its manager before adding a video.')),
      );
      return;
    }

    final controller = TextEditingController(text: initialUrl ?? '');
    String? errorText;
    bool inspecting = false;
    List<Map<String, dynamic>> formats = [];
    List<Map<String, dynamic>> audioFormats = [];
    Map<String, dynamic>? sourceInfo;
    String? inspectedSourceUrl;
    String? selectedFormatId;
    String? selectedAudioFormatId;

    final selection = await showDialog<_VideoFormatSelection>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setDialogState) {
          Future<void> inspectSource() async {
            if (inspecting) return;
            setDialogState(() {
              inspecting = true;
              errorText = null;
              formats = [];
              audioFormats = [];
              sourceInfo = null;
              selectedFormatId = null;
              selectedAudioFormatId = null;
            });
            try {
              final normalized = await rust_api.validateDownloadUrl(input: controller.text.trim());
              final String raw;
              if (Platform.isAndroid) {
                raw = (await _ytDlpAndroidChannel.invokeMethod<String>(
                  'inspectFormats',
                  {'sourceUrl': normalized},
                ))!;
              } else {
                final executable = await _installedYtDlpExecutable();
                raw = await rust_api.ytDlpInspectFormats(
                  executablePath: executable,
                  sourceUrl: normalized,
                );
              }
              final decoded = jsonDecode(raw) as Map<String, dynamic>;
              final reported = (decoded['formats'] as List<dynamic>? ?? [])
                  .whereType<Map<String, dynamic>>()
                  .toList();
              final available = reported.where((format) =>
                  format['hasVideo'] == true && format['hasAudio'] == true).toList();
              final availableAudio = reported.where((format) =>
                  format['hasVideo'] != true && format['hasAudio'] == true).toList();
              if (available.isEmpty && availableAudio.isEmpty) {
                throw StateError('This source did not report a directly downloadable video or audio format.');
              }
              if (!dialogContext.mounted) return;
              setDialogState(() {
                sourceInfo = decoded;
                inspectedSourceUrl = normalized;
                formats = available;
                audioFormats = availableAudio;
                selectedFormatId = available.isEmpty ? null : available.first['formatId'] as String?;
                selectedAudioFormatId = availableAudio.isEmpty ? null : availableAudio.first['formatId'] as String?;
              });
            } catch (error) {
              if (dialogContext.mounted) {
                setDialogState(() => errorText = error.toString().replaceFirst('Bad state: ', ''));
              }
            } finally {
              if (dialogContext.mounted) setDialogState(() => inspecting = false);
            }
          }

          String formatLabel(Map<String, dynamic> format) {
            final resolution = format['resolution'] as String? ?? 'Unknown quality';
            final extension = format['ext'] as String? ?? 'video';
            final fps = format['fps'] as num?;
            final size = (format['filesize'] ?? format['filesizeApprox']) as num?;
            final details = <String>[resolution, extension.toUpperCase()];
            if (fps != null) details.add('${fps.toStringAsFixed(0)} FPS');
            if (size != null) details.add(_formatYtDlpBytes(size.toInt()));
            return details.join(' • ');
          }

          String audioFormatLabel(Map<String, dynamic> format) {
            final extension = (format['ext'] as String? ?? 'audio').toUpperCase();
            final bitrate = format['audioBitrate'] as num?;
            final size = (format['filesize'] ?? format['filesizeApprox']) as num?;
            final details = <String>['Original audio', extension];
            if (bitrate != null) details.add('${bitrate.round()} kbps');
            if (size != null) details.add(_formatYtDlpBytes(size.toInt()));
            return details.join(' • ');
          }

          return AlertDialog(
            title: const Text('Choose video or audio quality'),
            content: SizedBox(
              width: 480,
              child: SingleChildScrollView(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    TextField(
                      controller: controller,
                      autofocus: true,
                      keyboardType: TextInputType.url,
                      decoration: const InputDecoration(
                        labelText: 'Video URL',
                        hintText: 'https://...',
                        border: OutlineInputBorder(),
                      ),
                      onChanged: (_) {
                        if (sourceInfo != null) {
                          setDialogState(() {
                            sourceInfo = null;
                            inspectedSourceUrl = null;
                            formats = [];
                            audioFormats = [];
                            selectedFormatId = null;
                            selectedAudioFormatId = null;
                          });
                        }
                      },
                      onSubmitted: (_) => inspectSource(),
                    ),
                    const SizedBox(height: 12),
                    OutlinedButton.icon(
                      onPressed: inspecting ? null : inspectSource,
                      icon: inspecting
                          ? const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2))
                          : const Icon(Icons.search_rounded),
                      label: const Text('Inspect available formats'),
                    ),
                    if (errorText != null) ...[
                      const SizedBox(height: 10),
                      Text(errorText!, style: const TextStyle(color: Color(0xFFFF8A80), fontSize: 12)),
                    ],
                    if (sourceInfo != null) ...[
                      const SizedBox(height: 14),
                      Text(sourceInfo!['title'] as String? ?? 'Video',
                          style: const TextStyle(fontWeight: FontWeight.w700)),
                      const SizedBox(height: 10),
                      if (formats.isNotEmpty) ...[
                        DropdownButtonFormField<String>(
                          value: selectedFormatId,
                          isExpanded: true,
                          decoration: const InputDecoration(
                            labelText: 'Video quality (video + audio)',
                            border: OutlineInputBorder(),
                          ),
                          items: formats.map((format) => DropdownMenuItem<String>(
                            value: format['formatId'] as String,
                            child: Text(formatLabel(format), overflow: TextOverflow.ellipsis),
                          )).toList(),
                          onChanged: (value) => setDialogState(() => selectedFormatId = value),
                        ),
                        const SizedBox(height: 10),
                      ],
                      if (audioFormats.isNotEmpty) ...[
                        DropdownButtonFormField<String>(
                          value: selectedAudioFormatId,
                          isExpanded: true,
                          decoration: const InputDecoration(
                            labelText: 'Audio only (original source format)',
                            border: OutlineInputBorder(),
                          ),
                          items: audioFormats.map((format) => DropdownMenuItem<String>(
                            value: format['formatId'] as String,
                            child: Text(audioFormatLabel(format), overflow: TextOverflow.ellipsis),
                          )).toList(),
                          onChanged: (value) => setDialogState(() => selectedAudioFormatId = value),
                        ),
                        const SizedBox(height: 8),
                      ],
                      const Text(
                        'Audio-only downloads keep the format supplied by the source. They are not MP3 conversions. Separate video-stream merging and MP3 conversion require the verified FFmpeg runtime.',
                        style: TextStyle(fontSize: 11, color: Color(0xFF9A9AAF)),
                      ),
                    ],
                  ],
                ),
              ),
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.of(dialogContext).pop(),
                child: const Text('Cancel'),
              ),
              if (formats.isNotEmpty)
                FilledButton.icon(
                  onPressed: sourceInfo != null && selectedFormatId != null && !inspecting
                      ? () {
                          final selected = formats.firstWhere(
                            (format) => format['formatId'] == selectedFormatId,
                          );
                          final extension = (selected['ext'] as String? ?? 'mp4')
                              .toLowerCase()
                              .replaceAll(RegExp(r'[^a-z0-9]'), '');
                          Navigator.of(dialogContext).pop(_VideoFormatSelection(
                            sourceUrl: Uri.parse(inspectedSourceUrl!),
                            title: sourceInfo!['title'] as String? ?? 'video',
                            formatId: selectedFormatId!,
                            extension: extension.isEmpty ? 'mp4' : extension,
                          ));
                        }
                      : null,
                  icon: const Icon(Icons.playlist_add_rounded),
                  label: const Text('Add video'),
                ),
              if (audioFormats.isNotEmpty)
                OutlinedButton.icon(
                  onPressed: sourceInfo != null && selectedAudioFormatId != null && !inspecting
                      ? () {
                          final selected = audioFormats.firstWhere(
                            (format) => format['formatId'] == selectedAudioFormatId,
                          );
                          final extension = (selected['ext'] as String? ?? 'm4a')
                              .toLowerCase()
                              .replaceAll(RegExp(r'[^a-z0-9]'), '');
                          Navigator.of(dialogContext).pop(_VideoFormatSelection(
                            sourceUrl: Uri.parse(inspectedSourceUrl!),
                            title: sourceInfo!['title'] as String? ?? 'audio',
                            formatId: selectedAudioFormatId!,
                            extension: extension.isEmpty ? 'm4a' : extension,
                          ));
                        }
                      : null,
                  icon: const Icon(Icons.audio_file_outlined),
                  label: const Text('Add original audio'),
                ),
            ],
          );
        },
      ),
    );

    controller.dispose();
    if (!mounted || selection == null) return;
    final duplicate = _links.any((link) =>
        link.url == selection.sourceUrl && link.ytDlpFormatId == selection.formatId);
    if (duplicate) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('This video and format are already in the queue.')),
      );
      return;
    }

    final safeTitle = sanitizeDownloadFileName(selection.title, fallback: 'video');
    final draft = _LinkDraft(
      url: selection.sourceUrl,
      fileName: '$safeTitle.${selection.extension}',
    )..ytDlpFormatId = selection.formatId;
    setState(() => _links.insert(0, draft));
    await _saveHistory();
    _pumpQueue();
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('Media added to the download queue.')),
    );
  }

  Future<void> _addLink() async {
    final controller = TextEditingController();
    String? errorText;

    final result = await showDialog<Uri>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setDialogState) {
          Future<void> validateAndSubmit() async {
            try {
              final normalized = await rust_api.validateDownloadUrl(
                input: controller.text.trim(),
              );
              final uri = Uri.parse(normalized);
              if (dialogContext.mounted) {
                Navigator.of(dialogContext).pop(uri);
              }
            } catch (_) {
              if (dialogContext.mounted) {
                setDialogState(() => errorText =
                    'Invalid URL. Enter a direct HTTP or HTTPS link.');
              }
            }
          }

          return AlertDialog(
            title: const Text('Add a direct link'),
            content: SizedBox(
              width: 440,
              child: TextField(
                controller: controller,
                autofocus: true,
                keyboardType: TextInputType.url,
                textInputAction: TextInputAction.done,
                onChanged: (_) {
                  if (errorText != null) {
                    setDialogState(() => errorText = null);
                  }
                },
                onSubmitted: (_) async => validateAndSubmit(),
                decoration: InputDecoration(
                  hintText: 'https://example.com/file.zip',
                  labelText: 'Download URL',
                  border: const OutlineInputBorder(),
                  errorText: errorText,
                ),
              ),
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.of(dialogContext).pop(),
                child: const Text('Cancel'),
              ),
              FilledButton.icon(
                onPressed: validateAndSubmit,
                icon: const Icon(Icons.link_rounded),
                label: const Text('Add link'),
              ),
            ],
          );
        },
      ),
    );

    controller.dispose();
    if (!mounted || result == null) return;

    final alreadyAdded = _links.any((link) => link.url.toString() == result.toString());
    if (alreadyAdded) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('This link is already in your download list.')),
      );
      return;
    }

    final segment = result.pathSegments.isEmpty
        ? ''
        : Uri.decodeComponent(result.pathSegments.last);
    final fileName = segment.isEmpty ? result.host : segment;

    setState(() {
      _links.insert(0, _LinkDraft(url: result, fileName: fileName));
    });
    await _saveHistory();

    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(
        content: Text('Link added to your download list.'),
      ),
    );
  }


  @override
  Widget build(BuildContext context) {
    const violet = EasyDownloadPalette.accent;
    const muted = Color(0xFF9A9AAF);
    const panel = Color(0xFF151525);
    final queuedCount = _links.where((link) =>
        !link.isDownloading && link.savedPath == null && !link.isFailed).length;
    final activeCount = _links.where((link) =>
        link.isDownloading && !link.isPaused).length;
    final downloadedCount = _links.where((link) => link.savedPath != null).length;
    final visibleLinks = _links.where((link) {
      final matchesFilter = _filter == 'All' ||
          (_filter == 'Active' && link.isDownloading) ||
          (_filter == 'Queued' && !link.isDownloading && link.savedPath == null) ||
          (_filter == 'Completed' && link.savedPath != null);
      final query = _searchQuery.trim().toLowerCase();
      final matchesSearch = query.isEmpty ||
          link.fileName.toLowerCase().contains(query) ||
          link.url.toString().toLowerCase().contains(query) ||
          (link.savedPath?.toLowerCase().contains(query) ?? false);
      final matchesFailed = !_showOnlyFailed || link.isFailed;
      final matchesPaused = !_showOnlyPaused || (link.isDownloading && link.isPaused);
      final matchesType = _typeFilter == 'All' ||
          (_typeFilter == 'Video' && _fileType(link.fileName) == 'Video') ||
          (_typeFilter == 'Audio' && _fileType(link.fileName) == 'Audio') ||
          (_typeFilter == 'Images' && _fileType(link.fileName) == 'Images') ||
          (_typeFilter == 'Archives' && _fileType(link.fileName) == 'Archives') ||
          (_typeFilter == 'Documents' && _fileType(link.fileName) == 'Documents') ||
          (_typeFilter == 'Other' && _fileType(link.fileName) == 'Other');
      return matchesFilter && matchesSearch && matchesType && matchesFailed && matchesPaused;
    }).toList();
    visibleLinks.sort((a, b) {
      if (_sortBy == 'Name') return a.fileName.toLowerCase().compareTo(b.fileName.toLowerCase());
      if (_sortBy == 'Largest') return (b.totalBytes ?? b.receivedBytes).compareTo(a.totalBytes ?? a.receivedBytes);
      if (_sortBy == 'Smallest') return (a.totalBytes ?? a.receivedBytes).compareTo(b.totalBytes ?? b.receivedBytes);
      return _links.indexOf(a).compareTo(_links.indexOf(b));
    });

    return Scaffold(
      appBar: AppBar(
        titleSpacing: 20,
        title: const Row(
          children: [
            Icon(Icons.downloading_rounded, color: violet, size: 29),
            SizedBox(width: 10),
            Text('Easy Download',
                style: TextStyle(fontWeight: FontWeight.w800)),
          ],
        ),
        actions: [
          IconButton(
            tooltip: 'Settings',
            onPressed: _showSettings,
            icon: const Icon(Icons.tune_rounded),
          ),
          const SizedBox(width: 8),
        ],
      ),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _addLink,
        icon: const Icon(Icons.add_rounded),
        label: const Text('Add download'),
      ),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.fromLTRB(18, 8, 18, 100),
          children: [
            Container(
              padding: const EdgeInsets.all(20),
              decoration: BoxDecoration(
                gradient: const LinearGradient(
                  begin: Alignment.topLeft,
                  end: Alignment.bottomRight,
                  colors: [Color(0xFF3A2418), Color(0xFF202020)],
                ),
                borderRadius: BorderRadius.circular(24),
                border: Border.all(color: violet.withValues(alpha: 0.25)),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Container(
                    padding: const EdgeInsets.symmetric(
                        horizontal: 10, vertical: 6),
                    decoration: BoxDecoration(
                      color: violet.withValues(alpha: 0.18),
                      borderRadius: BorderRadius.circular(30),
                    ),
                    child: const Text(
                      'DOWNLOAD MANAGER',
                      style: TextStyle(
                        color: Color(0xFFFFD3AD),
                        fontSize: 10,
                        letterSpacing: 1.2,
                        fontWeight: FontWeight.w800,
                      ),
                    ),
                  ),
                  const SizedBox(height: 16),
                  const Text(
                    'Everything you\nwant to keep.',
                    style: TextStyle(
                      fontSize: 30,
                      height: 1.12,
                      fontWeight: FontWeight.w800,
                    ),
                  ),
                  const SizedBox(height: 9),
                  const Text(
                    'Collect your download links in one simple place.',
                    style: TextStyle(color: muted, height: 1.4),
                  ),
                  const SizedBox(height: 18),
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      FilledButton.icon(
                        onPressed: _addLink,
                        icon: const Icon(Icons.add_link_rounded),
                        label: const Text('Add a link'),
                      ),
                      OutlinedButton.icon(
                        onPressed: _addVideoLink,
                        icon: const Icon(Icons.video_library_outlined),
                        label: const Text('Video quality'),
                      ),
                    ],
                  ),
                  if (_links.any((link) => link.isDownloading && link.isPaused)) ...[
                    const SizedBox(height: 8),
                    OutlinedButton.icon(
                      onPressed: _resumeAllPaused,
                      icon: const Icon(Icons.play_circle_outline_rounded),
                      label: const Text('Resume all paused'),
                    ),
                  ],
                  if (_links.any((link) => link.isDownloading && !link.isPaused)) ...[
                    const SizedBox(height: 8),
                    OutlinedButton.icon(
                      onPressed: _pauseAllActive,
                      icon: const Icon(Icons.pause_circle_outline_rounded),
                      label: const Text('Pause all active'),
                    ),
                  ],
                  if (_links.any((link) => !link.isDownloading && link.savedPath == null && !link.isFailed)) ...[
                    const SizedBox(height: 8),
                    OutlinedButton.icon(
                      onPressed: _downloadAllQueued,
                      icon: const Icon(Icons.playlist_play_rounded),
                      label: const Text('Download all queued'),
                    ),
                  ],
                ],
              ),
            ),
            const SizedBox(height: 18),
            _buildYtDlpManagerCard(),
            const SizedBox(height: 18),
            _buildFfmpegManagerCard(),
            const SizedBox(height: 18),
            Row(
              children: [
                Expanded(
                  child: _StatCard(
                    label: 'Active',
                    value: '$activeCount / $_maxConcurrentDownloads',
                    icon: Icons.downloading_rounded,
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: _StatCard(
                    label: 'In queue',
                    value: '$queuedCount',
                    icon: Icons.queue_rounded,
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: _StatCard(
                    label: 'Completed',
                    value: '$downloadedCount',
                    icon: Icons.check_circle_outline_rounded,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 26),
            Row(
              children: [
                const Text('Your links',
                    style: TextStyle(fontSize: 20, fontWeight: FontWeight.w800)),
                const Spacer(),
                if (_selectionMode) ...[
                  TextButton(
                    onPressed: () => setState(() {
                      _selectedLinks
                        ..clear()
                        ..addAll(visibleLinks.where((link) => !link.isDownloading));
                    }),
                    child: const Text('Select all'),
                  ),
                  TextButton(
                    onPressed: () => setState(() {
                      final selectable = visibleLinks.where((link) => !link.isDownloading).toSet();
                      final previouslySelected = _selectedLinks.toSet();
                      _selectedLinks
                        ..clear()
                        ..addAll(selectable.difference(previouslySelected));
                    }),
                    child: const Text('Invert'),
                  ),
                  TextButton(
                    onPressed: _copySelectedLinks,
                    child: const Text('Copy URLs'),
                  ),
                  TextButton(
                    onPressed: _removeSelectedLinks,
                    child: Text('Remove (${_selectedLinks.length})'),
                  ),
                  IconButton(
                    tooltip: 'Exit selection',
                    onPressed: () => setState(() {
                      _selectionMode = false;
                      _selectedLinks.clear();
                    }),
                    icon: const Icon(Icons.close_rounded),
                  ),
                ] else if (_links.isNotEmpty)
                  IconButton(
                    tooltip: 'Select downloads',
                    onPressed: () => setState(() => _selectionMode = true),
                    icon: const Icon(Icons.checklist_rounded),
                  ),
                if (_links.isNotEmpty)
                  IconButton(
                    tooltip: 'Paste links from clipboard',
                    onPressed: _pasteLinksFromClipboard,
                    icon: const Icon(Icons.content_paste_go_rounded),
                  ),
                if (_links.isNotEmpty)
                  IconButton(
                    tooltip: 'Export download list',
                    onPressed: _exportDownloadList,
                    icon: const Icon(Icons.ios_share_rounded),
                  ),
                if (_links.any((link) => !link.isDownloading))
                  TextButton.icon(
                    onPressed: _clearAllHistory,
                    icon: const Icon(Icons.delete_sweep_rounded, size: 16),
                    label: const Text('Clear history'),
                  )
                else if (_links.any((link) => !link.isDownloading && link.savedPath == null && !link.isFailed))
                  TextButton.icon(
                    onPressed: _clearQueued,
                    icon: const Icon(Icons.playlist_remove_rounded, size: 16),
                    label: const Text('Clear queued'),
                  )
                else if (_links.any((link) => link.isFailed && !link.isDownloading))
                  TextButton.icon(
                    onPressed: _clearFailed,
                    icon: const Icon(Icons.delete_sweep_outlined, size: 16),
                    label: const Text('Clear failed'),
                  )
                else if (downloadedCount > 0)
                  TextButton.icon(
                    onPressed: _clearCompleted,
                    icon: const Icon(Icons.cleaning_services_rounded, size: 16),
                    label: const Text('Clear completed'),
                  )
                else
                  Text(
                    queuedCount == 0 ? 'Nothing pending' : '$queuedCount pending',
                    style: const TextStyle(color: muted, fontSize: 12),
                  ),
              ],
            ),
            const SizedBox(height: 12),
            TextField(
              onChanged: (value) => setState(() => _searchQuery = value),
              decoration: InputDecoration(
                hintText: 'Search downloads',
                prefixIcon: const Icon(Icons.search_rounded),
                filled: true,
                fillColor: panel,
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(16),
                  borderSide: BorderSide.none,
                ),
              ),
            ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 4,
              children: [
                FilterChip(
                  label: const Text('Failed downloads only'),
                  selected: _showOnlyFailed,
                  onSelected: (selected) => setState(() => _showOnlyFailed = selected),
                  avatar: const Icon(Icons.error_outline_rounded, size: 18),
                ),
                FilterChip(
                  label: const Text('Paused downloads only'),
                  selected: _showOnlyPaused,
                  onSelected: (selected) => setState(() => _showOnlyPaused = selected),
                  avatar: const Icon(Icons.pause_circle_outline_rounded, size: 18),
                ),
              ],
            ),
            const SizedBox(height: 8),
            SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: Row(
                children: ['Newest', 'Name', 'Largest', 'Smallest'].map((sort) {
                  final selected = _sortBy == sort;
                  return Padding(
                    padding: const EdgeInsets.only(right: 8),
                    child: ChoiceChip(
                      label: Text('Sort: $sort'),
                      selected: selected,
                      onSelected: (_) => setState(() => _sortBy = sort),
                    ),
                  );
                }).toList(),
              ),
            ),
            const SizedBox(height: 10),
            SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: Row(
                children: ['All', 'Video', 'Audio', 'Images', 'Archives', 'Documents', 'Other'].map((type) {
                  final selected = _typeFilter == type;
                  return Padding(
                    padding: const EdgeInsets.only(right: 8),
                    child: ChoiceChip(
                      label: Text(type),
                      selected: selected,
                      onSelected: (_) => setState(() => _typeFilter = type),
                    ),
                  );
                }).toList(),
              ),
            ),
            const SizedBox(height: 10),
            SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: Row(
                children: ['All', 'Active', 'Queued', 'Completed'].map((filter) {
                  final selected = _filter == filter;
                  return Padding(
                    padding: const EdgeInsets.only(right: 8),
                    child: ChoiceChip(
                      label: Text(filter),
                      selected: selected,
                      onSelected: (_) => setState(() => _filter = filter),
                    ),
                  );
                }).toList(),
              ),
            ),
            const SizedBox(height: 12),
            if (_links.isEmpty)
              const _EmptyState()
            else if (visibleLinks.isEmpty)
              const Padding(
                padding: EdgeInsets.symmetric(vertical: 28),
                child: Center(child: Text('No downloads match this view.')),
              )
            else
              ...visibleLinks.map((link) => Padding(
                    padding: const EdgeInsets.only(bottom: 10),
                    child: Container(
                      decoration: BoxDecoration(
                        color: panel,
                        borderRadius: BorderRadius.circular(18),
                        border: Border.all(
                            color: Colors.white.withValues(alpha: 0.06)),
                      ),
                      child: ListTile(
                        contentPadding: const EdgeInsets.symmetric(
                            horizontal: 14, vertical: 8),
                        leading: _selectionMode
                            ? Checkbox(
                                value: _selectedLinks.contains(link),
                                onChanged: link.isDownloading
                                    ? null
                                    : (checked) => setState(() {
                                          if (checked == true) {
                                            _selectedLinks.add(link);
                                          } else {
                                            _selectedLinks.remove(link);
                                          }
                                        }),
                              )
                            : Container(
                          width: 48,
                          height: 48,
                          decoration: BoxDecoration(
                            color: _downloadAccentForName(link.fileName)
                                .withValues(alpha: 0.14),
                            borderRadius: BorderRadius.circular(15),
                            border: Border.all(
                              color: _downloadAccentForName(link.fileName)
                                  .withValues(alpha: 0.28),
                            ),
                          ),
                          child: Icon(
                            _downloadIconForName(link.fileName),
                            color: _downloadAccentForName(link.fileName),
                            size: 25,
                          ),
                        ),
                        title: InkWell(
                          onLongPress: () => _renameDownload(link),
                          onTap: () => _showDownloadDetails(link),
                          borderRadius: BorderRadius.circular(4),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Text(
                                link.fileName,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: const TextStyle(fontWeight: FontWeight.w700),
                              ),
                              const SizedBox(height: 5),
                              Container(
                                padding: const EdgeInsets.symmetric(
                                  horizontal: 7,
                                  vertical: 2,
                                ),
                                decoration: BoxDecoration(
                                  color: _downloadAccentForName(link.fileName)
                                      .withValues(alpha: 0.12),
                                  borderRadius: BorderRadius.circular(6),
                                ),
                                child: Text(
                                  _downloadTypeLabel(link.fileName),
                                  style: TextStyle(
                                    color: _downloadAccentForName(link.fileName),
                                    fontSize: 9,
                                    fontWeight: FontWeight.w800,
                                    letterSpacing: 0.7,
                                  ),
                                ),
                              ),
                            ],
                          ),
                        ),
                        subtitle: Padding(
                          padding: const EdgeInsets.only(top: 5),
                          child: link.isDownloading
                              ? Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    Text(
                                      '${_formatBytes(link.receivedBytes)} / ${link.totalBytes == null || link.totalBytes == 0 ? 'Unknown size' : _formatBytes(link.totalBytes!)}',
                                      style: const TextStyle(color: muted, fontSize: 12),
                                    ),
                                    const SizedBox(height: 5),
                                    LinearProgressIndicator(
                                      value: link.progressPercent != null
                                          ? (link.progressPercent! / 100).clamp(0.0, 1.0)
                                          : link.totalBytes != null && link.totalBytes! > 0
                                              ? (link.receivedBytes / link.totalBytes!).clamp(0.0, 1.0)
                                              : null,
                                      minHeight: 4,
                                      borderRadius: BorderRadius.circular(8),
                                    ),
                                    const SizedBox(height: 4),
                                    Text(
                                      link.progressPercent != null
                                          ? 'yt-dlp  •  ${link.progressPercent!.toStringAsFixed(0)}%${link.progressEtaSeconds != null && link.progressEtaSeconds! > 0 ? '  •  ${_formatRemaining(link.progressEtaSeconds!)}' : ''}'
                                          : '${_formatBytes(link.speedBytesPerSecond.round())}/s  •  ${link.totalBytes != null && link.totalBytes! > 0 ? (link.receivedBytes * 100 / link.totalBytes!).toStringAsFixed(0) + '%' : 'Calculating…'}${link.totalBytes != null && link.totalBytes! > link.receivedBytes && link.speedBytesPerSecond > 0 ? '  •  ${_formatRemaining(((link.totalBytes! - link.receivedBytes) / link.speedBytesPerSecond).ceil())}' : ''}',
                                      style: const TextStyle(color: muted, fontSize: 11),
                                    ),
                                    if (link.isPaused)
                                      const Padding(
                                        padding: EdgeInsets.only(top: 4),
                                        child: Text('Paused', style: TextStyle(color: Colors.amber, fontSize: 11, fontWeight: FontWeight.w600)),
                                      ),
                                  ],
                                )
                              : link.savedPath != null
                                  ? Column(
                                      crossAxisAlignment: CrossAxisAlignment.start,
                                      mainAxisSize: MainAxisSize.min,
                                      children: [
                                        Text(
                                          _formatBytes(link.receivedBytes),
                                          style: const TextStyle(color: muted, fontSize: 11, fontWeight: FontWeight.w600),
                                        ),
                                        const SizedBox(height: 3),
                                        Text(
                                          link.savedPath!,
                                          maxLines: 2,
                                          overflow: TextOverflow.ellipsis,
                                          style: const TextStyle(color: muted, fontSize: 12),
                                        ),
                                      ],
                                    )
                                  : Column(
                                      crossAxisAlignment: CrossAxisAlignment.start,
                                      mainAxisSize: MainAxisSize.min,
                                      children: [
                                        if (!link.isFailed && link.savedPath == null)
                                          Text(
                                            'Queue position: #${_queuePosition(link)}',
                                            style: const TextStyle(
                                              color: Color(0xFFFFD3AD),
                                              fontSize: 11,
                                              fontWeight: FontWeight.w700,
                                            ),
                                          ),
                                        Text(
                                          link.url.toString(),
                                          maxLines: 2,
                                          overflow: TextOverflow.ellipsis,
                                          style: const TextStyle(color: muted, fontSize: 12),
                                        ),
                                      ],
                                    ),
                        ),
                        trailing: link.savedPath != null
                            ? Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  const Icon(Icons.check_circle_rounded, color: Color(0xFF4ADE80)),
                                  IconButton(
                                    tooltip: 'Copy saved path',
                                    onPressed: () => _copySavedPath(link),
                                    icon: const Icon(Icons.copy_rounded),
                                  ),
                                  IconButton(
                                    tooltip: 'Remove from list',
                                    onPressed: () => _removeLink(link),
                                    icon: const Icon(Icons.delete_outline_rounded),
                                  ),
                                ],
                              )
                            : link.isDownloading
                                ? Row(
                                    mainAxisSize: MainAxisSize.min,
                                    children: [
                                      IconButton(
                                        tooltip: link.isPaused ? 'Resume' : 'Pause',
                                        onPressed: () => link.isPaused
                                            ? _resumeDownload(link)
                                            : _pauseDownload(link),
                                        icon: Icon(
                                          link.isPaused
                                              ? Icons.play_arrow_rounded
                                              : Icons.pause_rounded,
                                          color: violet,
                                        ),
                                      ),
                                      IconButton(
                                        tooltip: 'Cancel download',
                                        onPressed: () => _cancelDownload(link),
                                        icon: const Icon(Icons.close_rounded),
                                      ),
                                    ],
                                  )
                                : Row(
                                    mainAxisSize: MainAxisSize.min,
                                    children: [
                                      IconButton(
                                        tooltip: 'Copy download URL',
                                        onPressed: () => _copyDownloadUrl(link),
                                        icon: const Icon(Icons.link_rounded),
                                      ),
                                      IconButton(
                                        tooltip: link.isFailed ? 'Retry download' : 'Download',
                                        onPressed: () => _enqueueDownload(link),
                                        icon: Icon(link.isFailed ? Icons.refresh_rounded : Icons.download_rounded, color: violet),
                                      ),
                                      if (!link.isFailed)
                                        PopupMenuButton<int>(
                                          tooltip: 'Change queue position',
                                          onSelected: (direction) =>
                                              _moveQueuedDownload(link, direction),
                                          itemBuilder: (context) => [
                                            PopupMenuItem<int>(
                                              value: -1,
                                              enabled: _queuePosition(link) > 1,
                                              child: const Text('Move up in queue'),
                                            ),
                                            PopupMenuItem<int>(
                                              value: 1,
                                              enabled: _queuePosition(link) < queuedCount,
                                              child: const Text('Move down in queue'),
                                            ),
                                          ],
                                        ),
                                      IconButton(
                                        tooltip: 'Remove from list',
                                        onPressed: () => _removeLink(link),
                                        icon: const Icon(Icons.delete_outline_rounded),
                                      ),
                                    ],
                                  ),
                      ),
                    ),
                  )),
            const SizedBox(height: 18),
            Container(
              padding: const EdgeInsets.all(15),
              decoration: BoxDecoration(
                color: panel,
                borderRadius: BorderRadius.circular(18),
              ),
              child: const Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.info_outline_rounded, color: violet, size: 21),
                  SizedBox(width: 12),
                  Expanded(
                    child: Text(
                      'Downloads save inside app storage. Use pause, resume, or cancel while a download is active.',
                      style: TextStyle(color: muted, height: 1.45, fontSize: 12),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _showSettings() async {
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('Download settings'),
        content: Text(Platform.isAndroid
            ? (_androidDownloadTreeUri == null
                ? 'Downloads are saved in Easy Download app storage. Choose a folder to save completed downloads there. Downloads are staged privately while in progress, then copied to your chosen folder.'
                : 'A folder has been selected for completed downloads. In-progress files remain in private app storage so pause and resume continue to work.')
            : 'Download folder:\n${_downloadDirectoryPath ?? 'App documents folder (default)'}'),
        actions: [
          TextButton.icon(
            onPressed: () {
              Navigator.of(dialogContext).pop();
              _chooseDownloadDirectory();
            },
            icon: const Icon(Icons.folder_open_rounded),
            label: const Text('Choose folder'),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: const Text('Close'),
          ),
        ],
      ),
    );
  }

  Future<String> _copyToAndroidDownloadDirectory(String sourcePath, String fileName) async {
    final treeUri = _androidDownloadTreeUri;
    if (treeUri == null) return sourcePath;
    final savedUri = await _storageChannel.invokeMethod<String>(
      'copyFileToDownloadDirectory',
      {'treeUri': treeUri, 'sourcePath': sourcePath, 'fileName': fileName},
    );
    if (savedUri == null || savedUri.isEmpty) {
      throw StateError('Android did not return the saved file location.');
    }
    return savedUri;
  }

  Future<void> _chooseDownloadDirectory() async {
    try {
      if (Platform.isAndroid) {
        final selectedUri = await _storageChannel.invokeMethod<String>('pickDownloadDirectory');
        if (selectedUri == null || !mounted) return;
        final prefs = await SharedPreferences.getInstance();
        await prefs.setString(_androidDirectoryKey, selectedUri);
        if (!mounted) return;
        setState(() => _androidDownloadTreeUri = selectedUri);
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Android download folder updated.')),
        );
        return;
      }
      final selected = await FilePicker.getDirectoryPath(
        dialogTitle: 'Choose download folder',
        initialDirectory: _downloadDirectoryPath,
      );
      if (selected == null || !mounted) return;
      final directory = Directory(selected);
      if (!await directory.exists()) {
        throw StateError('The selected folder is not available.');
      }
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(_downloadDirectoryKey, selected);
      if (!mounted) return;
      setState(() => _downloadDirectoryPath = selected);
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Download folder updated. New downloads will use this folder.')),
      );
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Could not select download folder: $error')),
      );
    }
  }
}

class _StatCard extends StatelessWidget {
  const _StatCard(
      {required this.label, required this.value, required this.icon});

  final String label;
  final String value;
  final IconData icon;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(icon, color: EasyDownloadPalette.accent, size: 22),
            const SizedBox(height: 18),
            Text(value,
                style:
                    const TextStyle(fontSize: 26, fontWeight: FontWeight.w800)),
            const SizedBox(height: 3),
            Text(label,
                style: const TextStyle(
                    color: Color(0xFF9A9AAF), fontSize: 13)),
          ],
        ),
      ),
    );
  }
}

class _EmptyState extends StatelessWidget {
  const _EmptyState();

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 28),
        child: Column(
          children: [
            Container(
              width: 64,
              height: 64,
              decoration: BoxDecoration(
                color: EasyDownloadPalette.accent.withValues(alpha: 0.14),
                shape: BoxShape.circle,
              ),
              child: const Icon(Icons.download_for_offline_outlined,
                  size: 32, color: EasyDownloadPalette.accent),
            ),
            const SizedBox(height: 14),
            const Text('Nothing downloading yet',
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.w700)),
            const SizedBox(height: 6),
            const Text(
              'Add a direct link to place it in your session list.',
              textAlign: TextAlign.center,
              style: TextStyle(color: Color(0xFF9A9AAF), height: 1.4),
            ),
          ],
        ),
      ),
    );
  }
}
