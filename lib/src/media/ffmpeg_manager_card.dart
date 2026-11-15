import 'dart:io';

import 'package:flutter/material.dart';

import 'ffmpeg_manager_config.dart';

/// Presentation-only FFmpeg manager card.
///
/// All operations are injected by the owning page so the widget can be tested
/// directly without invoking platform channels or the native FFmpeg engine.
class FfmpegManagerCard extends StatelessWidget {
  const FfmpegManagerCard({
    super.key,
    required this.supported,
    required this.installedVersion,
    required this.latestVersion,
    required this.lastCheckedAt,
    required this.executablePath,
    required this.status,
    required this.checking,
    required this.installing,
    required this.receivedBytes,
    required this.totalBytes,
    required this.converting,
    required this.paused,
    required this.inputPath,
    required this.bitrate,
    required this.elapsedMillis,
    required this.totalMillis,
    required this.autoCheckEnabled,
    required this.history,
    required this.onChooseExecutable,
    required this.onCheckUpdates,
    required this.onInstallLatest,
    required this.onRollback,
    required this.onVerify,
    required this.onChooseVideo,
    required this.onBitrateChanged,
    required this.onConvert,
    required this.onPause,
    required this.onResume,
    required this.onCancel,
    required this.onAutoCheckChanged,
  });

  final bool supported;
  final String? installedVersion;
  final String? latestVersion;
  final String? lastCheckedAt;
  final String? executablePath;
  final String status;
  final bool checking;
  final bool installing;
  final int receivedBytes;
  final int? totalBytes;
  final bool converting;
  final bool paused;
  final String? inputPath;
  final String bitrate;
  final int elapsedMillis;
  final int? totalMillis;
  final bool autoCheckEnabled;
  final List<String> history;
  final VoidCallback onChooseExecutable;
  final VoidCallback onCheckUpdates;
  final VoidCallback onInstallLatest;
  final VoidCallback onRollback;
  final VoidCallback onVerify;
  final VoidCallback onChooseVideo;
  final ValueChanged<String?> onBitrateChanged;
  final VoidCallback onConvert;
  final VoidCallback onPause;
  final VoidCallback onResume;
  final VoidCallback onCancel;
  final ValueChanged<bool> onAutoCheckChanged;

  String _formatBytes(int bytes) {
    if (bytes < 1024) return '$bytes B';
    if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(1)} KB';
    return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
  }

  String _timeLabel(int millis) {
    final seconds = millis ~/ 1000;
    final minutes = seconds ~/ 60;
    return minutes.toString().padLeft(2, '0') +
        ':' +
        (seconds % 60).toString().padLeft(2, '0');
  }

  @override
  Widget build(BuildContext context) {
    final progress = totalMillis != null && totalMillis! > 0
        ? (elapsedMillis / totalMillis!).clamp(0.0, 1.0).toDouble()
        : null;
    final canInstall = supported &&
        !converting &&
        !checking &&
        !installing &&
        latestVersion != null &&
        latestVersion != installedVersion;

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                const Icon(Icons.graphic_eq_rounded,
                    color: Color(0xFF60A5FA), size: 25),
                const SizedBox(width: 10),
                const Expanded(
                  child: Text('FFmpeg manager',
                      style:
                          TextStyle(fontSize: 17, fontWeight: FontWeight.w800)),
                ),
                Container(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 9, vertical: 5),
                  decoration: BoxDecoration(
                    color: supported
                        ? const Color(0xFF16A34A).withValues(alpha: 0.14)
                        : Colors.orange.withValues(alpha: 0.14),
                    borderRadius: BorderRadius.circular(20),
                  ),
                  child: Text(supported ? 'DESKTOP' : 'NOT AVAILABLE',
                      style: TextStyle(
                        color: supported
                            ? const Color(0xFF4ADE80)
                            : Colors.orangeAccent,
                        fontSize: 10,
                        fontWeight: FontWeight.w800,
                      )),
                ),
              ],
            ),
            const SizedBox(height: 12),
            Text('Installed: ${installedVersion ?? 'Not installed'}'),
            Text('Latest official: ${latestVersion ?? 'Not checked'}'),
            Text(
              'Last check: ${lastCheckedAt == null ? 'Not checked yet' : (DateTime.tryParse(lastCheckedAt!)?.toLocal().toString().substring(0, 16) ?? 'Unknown')}',
              style: const TextStyle(color: Color(0xFF9A9AAF), fontSize: 12),
            ),
            if (executablePath != null) ...[
              const SizedBox(height: 4),
              Text(executablePath!,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style:
                      const TextStyle(color: Color(0xFF9A9AAF), fontSize: 11)),
            ],
            const SizedBox(height: 10),
            Text(status,
                style: const TextStyle(color: Color(0xFFBDBDCE))),
            if (installing) ...[
              const SizedBox(height: 10),
              LinearProgressIndicator(
                value: totalBytes != null && totalBytes! > 0
                    ? (receivedBytes / totalBytes!).clamp(0.0, 1.0).toDouble()
                    : null,
              ),
              const SizedBox(height: 5),
              Text(
                totalBytes == null
                    ? _formatBytes(receivedBytes)
                    : '${_formatBytes(receivedBytes)} / ${_formatBytes(totalBytes!)}',
                style: const TextStyle(
                    fontSize: 12, color: Color(0xFF9A9AAF)),
              ),
            ],
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                OutlinedButton.icon(
                  onPressed: supported && !converting ? onChooseExecutable : null,
                  icon: const Icon(Icons.folder_open_rounded),
                  label: Text(installedVersion == null
                      ? 'Choose FFmpeg'
                      : 'Change FFmpeg'),
                ),
                OutlinedButton.icon(
                  onPressed: supported &&
                          !converting &&
                          !checking &&
                          !installing
                      ? onCheckUpdates
                      : null,
                  icon: checking
                      ? const SizedBox(
                          width: 16,
                          height: 16,
                          child: CircularProgressIndicator(strokeWidth: 2))
                      : const Icon(Icons.refresh_rounded),
                  label: const Text('Check updates'),
                ),
                FilledButton.icon(
                  onPressed: canInstall ? onInstallLatest : null,
                  icon: const Icon(Icons.system_update_alt_rounded),
                  label: Text(installing ? 'Updating...' : 'Install / Update'),
                ),
                OutlinedButton.icon(
                  onPressed:
                      supported && !converting && !installing ? onRollback : null,
                  icon: const Icon(Icons.history_rounded),
                  label: const Text('Rollback'),
                ),
                OutlinedButton.icon(
                  onPressed: supported && !converting && executablePath != null
                      ? onVerify
                      : null,
                  icon: const Icon(Icons.verified_outlined),
                  label: const Text('Verify'),
                ),
              ],
            ),
            const Divider(height: 24),
            const Text('Video to MP3',
                style: TextStyle(fontWeight: FontWeight.w800)),
            const SizedBox(height: 6),
            OutlinedButton.icon(
              onPressed: supported && !converting ? onChooseVideo : null,
              icon: const Icon(Icons.video_file_outlined),
              label: Text(inputPath == null
                  ? 'Choose video file'
                  : inputPath!.split(RegExp(r'[\\/]')).last),
            ),
            const SizedBox(height: 10),
            DropdownButtonFormField<String>(
              value: bitrate,
              decoration: const InputDecoration(
                labelText: 'MP3 quality',
                border: OutlineInputBorder(),
                isDense: true,
              ),
              items: ffmpegMp3Bitrates
                  .map((rate) =>
                      DropdownMenuItem(value: rate, child: Text(rate)))
                  .toList(),
              onChanged:
                  supported && !converting ? onBitrateChanged : null,
            ),
            if (converting) ...[
              const SizedBox(height: 12),
              LinearProgressIndicator(value: progress),
              const SizedBox(height: 5),
              Text(
                totalMillis == null
                    ? _timeLabel(elapsedMillis)
                    : '${_timeLabel(elapsedMillis)} / ${_timeLabel(totalMillis!)}',
                style: const TextStyle(
                    fontSize: 12, color: Color(0xFF9A9AAF)),
              ),
            ],
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                FilledButton.icon(
                  onPressed: supported &&
                          !converting &&
                          executablePath != null &&
                          inputPath != null
                      ? onConvert
                      : null,
                  icon: const Icon(Icons.music_note_rounded),
                  label: const Text('Convert to MP3'),
                ),
                if (converting)
                  OutlinedButton.icon(
                    onPressed: paused ? onResume : onPause,
                    icon: Icon(paused
                        ? Icons.play_arrow_rounded
                        : Icons.pause_rounded),
                    label: Text(paused ? 'Resume' : 'Pause'),
                  ),
                if (converting)
                  OutlinedButton.icon(
                    onPressed: onCancel,
                    icon: const Icon(Icons.close_rounded),
                    label: const Text('Cancel'),
                  ),
              ],
            ),
            if (supported) ...[
              SwitchListTile.adaptive(
                contentPadding: EdgeInsets.zero,
                title: const Text('Automatic update checks'),
                subtitle: const Text(
                    'Check the official release every 24 hours while Easy Download is open.'),
                value: autoCheckEnabled,
                onChanged: !installing && !checking ? onAutoCheckChanged : null,
              ),
              if (history.isNotEmpty) ...[
                const SizedBox(height: 4),
                const Text('Recent FFmpeg updates',
                    style: TextStyle(fontWeight: FontWeight.w700)),
                const SizedBox(height: 4),
                ...history.take(3).map((entry) => Padding(
                      padding: const EdgeInsets.only(bottom: 3),
                      child: Text(entry,
                          style: const TextStyle(
                              fontSize: 11, color: Color(0xFF9A9AAF))),
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
}
