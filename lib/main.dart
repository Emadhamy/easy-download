import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'dart:convert';
import 'dart:io';
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
    const background = Color(0xFF0B0B14);
    const panel = Color(0xFF151525);
    const violet = Color(0xFF8B5CF6);

    return MaterialApp(
      title: 'Easy Download',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        useMaterial3: true,
        brightness: Brightness.dark,
        scaffoldBackgroundColor: background,
        colorScheme: ColorScheme.fromSeed(
          seedColor: violet,
          brightness: Brightness.dark,
          surface: panel,
        ),
      ),
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

class _LinkDraft {
  _LinkDraft({required this.url, required this.fileName});
  final Uri url;
  final String fileName;
  bool isDownloading = false;
  bool isPaused = false;
  bool isCancelled = false;
  bool isFailed = false;
  String? taskId;
  int receivedBytes = 0;
  int? totalBytes;
  double speedBytesPerSecond = 0;
  String? savedPath;

  Map<String, dynamic> toJson() => {
        'url': url.toString(),
        'fileName': fileName,
        'savedPath': savedPath,
      };

  static _LinkDraft? fromJson(Map<String, dynamic> json) {
    final parsed = Uri.tryParse(json['url'] as String? ?? '');
    if (parsed == null || !parsed.hasScheme || parsed.host.isEmpty) return null;
    return _LinkDraft(url: parsed, fileName: json['fileName'] as String? ?? parsed.host)
      ..savedPath = json['savedPath'] as String?;
  }
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
  static const _historyKey = 'easy_download_history_v1';

  @override
  void initState() {
    super.initState();
    _loadHistory();
  }

  Future<void> _loadHistory() async {
    final prefs = await SharedPreferences.getInstance();
    final raw = prefs.getString(_historyKey);
    if (raw == null || !mounted) return;
    try {
      final entries = jsonDecode(raw) as List<dynamic>;
      final restored = entries
          .whereType<Map<String, dynamic>>()
          .map(_LinkDraft.fromJson)
          .whereType<_LinkDraft>()
          .toList();
      setState(() { _links..clear()..addAll(restored); });
    } catch (_) {
      // Keep the app usable if local history is invalid.
    }
  }

  Future<void> _saveHistory() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_historyKey, jsonEncode(_links.map((item) => item.toJson()).toList()));
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

  Future<void> _removeLink(_LinkDraft link) async {
    if (link.isDownloading) return;
    setState(() => _links.remove(link));
    await _saveHistory();
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

  Future<void> _download(_LinkDraft link) async {
    if (link.isDownloading || link.savedPath != null) return;
    final taskId = 'download-${DateTime.now().microsecondsSinceEpoch}';
    link.taskId = taskId;
    setState(() {
      link.isDownloading = true;
      link.isPaused = false;
      link.isCancelled = false;
      link.isFailed = false;
      link.receivedBytes = 0;
      link.totalBytes = null;
      link.speedBytesPerSecond = 0;
    });
    var previousBytes = 0;
    var previousTime = DateTime.now();
    try {
      final directory = await getApplicationDocumentsDirectory();
      final rawName = link.fileName.replaceAll('/', '_').replaceAll('\\\\', '_');
      final safeName = rawName.trim().isEmpty || rawName == '.' || rawName == '..'
          ? 'download-${DateTime.now().millisecondsSinceEpoch}'
          : rawName;
      // Replace characters that are invalid in common desktop filesystems.
      final portableName = safeName.replaceAll(RegExp(r'[<>:"|?*]'), '_');
      final finalName = portableName.trim().isEmpty || portableName == '.' || portableName == '..'
          ? 'download-${DateTime.now().millisecondsSinceEpoch}'
          : portableName;
      // Avoid overwriting an existing file with the same name.
      var destination = directory.path + '/' + finalName;
      var candidateName = finalName;
      var suffix = 1;
      while (await File(destination).exists()) {
        final dot = finalName.lastIndexOf('.');
        final hasExtension = dot > 0;
        final baseName = hasExtension ? finalName.substring(0, dot) : finalName;
        final extension = hasExtension ? finalName.substring(dot) : '';
        candidateName = '$baseName ($suffix)$extension';
        destination = directory.path + '/' + candidateName;
        suffix++;
      }
      await for (final event in rust_api.downloadFileControlled(
        taskId: taskId,
        inputUrl: link.url.toString(),
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
          link.speedBytesPerSecond = speed;
        });
      }
      if (!mounted) return;
      setState(() { link.savedPath = destination; link.isDownloading = false; link.isPaused = false; link.taskId = null; });
      await _saveHistory();
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('Saved to app storage: $candidateName')));
    } catch (_) {
      if (!mounted) return;
      setState(() { link.isDownloading = false; link.isPaused = false; link.taskId = null; link.isFailed = !link.isCancelled; });
      if (!link.isCancelled) {
        ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('Download failed. Check the link and connection.')));
      }
    }
  }


  Future<void> _retryDownload(_LinkDraft link) async {
    if (link.isDownloading || link.savedPath != null) return;
    await _download(link);
  }

  Future<void> _pauseDownload(_LinkDraft link) async {
    final taskId = link.taskId;
    if (taskId == null || !link.isDownloading || link.isPaused) return;
    try {
      await rust_api.pauseDownload(taskId: taskId);
      if (mounted) setState(() => link.isPaused = true);
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
    try {
      await rust_api.cancelDownload(taskId: taskId);
      link.isCancelled = true;
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
      if (mounted) {
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
    const violet = Color(0xFF8B5CF6);
    const muted = Color(0xFF9A9AAF);
    const panel = Color(0xFF151525);
    final queuedCount = _links.where((link) => link.savedPath == null).length;
    final downloadedCount = _links.where((link) => link.savedPath != null).length;
    final visibleLinks = _links.where((link) {
      final matchesFilter = _filter == 'All' ||
          (_filter == 'Active' && link.isDownloading) ||
          (_filter == 'Queued' && !link.isDownloading && link.savedPath == null) ||
          (_filter == 'Completed' && link.savedPath != null);
      final query = _searchQuery.trim().toLowerCase();
      final matchesSearch = query.isEmpty ||
          link.fileName.toLowerCase().contains(query) ||
          link.url.toString().toLowerCase().contains(query);
      final matchesType = _typeFilter == 'All' ||
          (_typeFilter == 'Video' && _fileType(link.fileName) == 'Video') ||
          (_typeFilter == 'Audio' && _fileType(link.fileName) == 'Audio') ||
          (_typeFilter == 'Images' && _fileType(link.fileName) == 'Images') ||
          (_typeFilter == 'Archives' && _fileType(link.fileName) == 'Archives') ||
          (_typeFilter == 'Documents' && _fileType(link.fileName) == 'Documents') ||
          (_typeFilter == 'Other' && _fileType(link.fileName) == 'Other');
      return matchesFilter && matchesSearch && matchesType;
    }).toList();

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
            onPressed: () => _showNotice(context),
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
                  colors: [Color(0xFF29204A), Color(0xFF171727)],
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
                        color: Color(0xFFD4C5FF),
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
                  FilledButton.icon(
                    onPressed: _addLink,
                    icon: const Icon(Icons.add_link_rounded),
                    label: const Text('Add a link'),
                  ),
                ],
              ),
            ),
            const SizedBox(height: 18),
            Row(
              children: [
                Expanded(
                  child: _StatCard(
                    label: 'Queued links',
                    value: '$queuedCount',
                    icon: Icons.link_rounded,
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: _StatCard(
                    label: 'Downloaded',
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
                if (downloadedCount > 0)
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
                        leading: Container(
                          width: 46,
                          height: 46,
                          decoration: BoxDecoration(
                            color: violet.withValues(alpha: 0.15),
                            borderRadius: BorderRadius.circular(14),
                          ),
                          child: Icon(_downloadIconForName(link.fileName),
                              color: violet),
                        ),
                        title: Text(link.fileName,
                            maxLines: 1, overflow: TextOverflow.ellipsis,
                            style: const TextStyle(fontWeight: FontWeight.w700)),
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
                                      value: link.totalBytes != null && link.totalBytes! > 0
                                          ? (link.receivedBytes / link.totalBytes!).clamp(0.0, 1.0)
                                          : null,
                                      minHeight: 4,
                                      borderRadius: BorderRadius.circular(8),
                                    ),
                                    const SizedBox(height: 4),
                                    Text(
                                      '${_formatBytes(link.speedBytesPerSecond.round())}/s  •  ${link.totalBytes != null && link.totalBytes! > 0 ? (link.receivedBytes * 100 / link.totalBytes!).toStringAsFixed(0) + '%' : 'Calculating…'}${link.totalBytes != null && link.totalBytes! > link.receivedBytes && link.speedBytesPerSecond > 0 ? '  •  ${_formatRemaining(((link.totalBytes! - link.receivedBytes) / link.speedBytesPerSecond).ceil())}' : ''}',
                                      style: const TextStyle(color: muted, fontSize: 11),
                                    ),
                                    if (link.isPaused)
                                      const Padding(
                                        padding: EdgeInsets.only(top: 4),
                                        child: Text('Paused', style: TextStyle(color: Colors.amber, fontSize: 11, fontWeight: FontWeight.w600)),
                                      ),
                                  ],
                                )
                              : Text(
                                  link.savedPath ?? link.url.toString(),
                                  maxLines: 2,
                                  overflow: TextOverflow.ellipsis,
                                  style: const TextStyle(color: muted, fontSize: 12),
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
                                        tooltip: link.isFailed ? 'Retry download' : 'Download',
                                        onPressed: () => link.isFailed ? _retryDownload(link) : _download(link),
                                        icon: Icon(link.isFailed ? Icons.refresh_rounded : Icons.download_rounded, color: violet),
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

  static void _showNotice(BuildContext context) {
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('Settings will be added in a later step.')),
    );
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
            Icon(icon, color: const Color(0xFF8B5CF6), size: 22),
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
                color: const Color(0xFF8B5CF6).withValues(alpha: 0.14),
                shape: BoxShape.circle,
              ),
              child: const Icon(Icons.download_for_offline_outlined,
                  size: 32, color: Color(0xFF8B5CF6)),
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
