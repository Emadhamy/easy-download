import 'package:flutter/material.dart';

import 'media_models.dart';

/// Presentation-only format picker. The engine is connected in a later step.
class MediaFormatPicker extends StatefulWidget {
  const MediaFormatPicker({
    super.key,
    required this.inspection,
    required this.outputPath,
    required this.onQueue,
  });

  final MediaInspection inspection;
  final String outputPath;
  final ValueChanged<MediaTaskRequest> onQueue;

  @override
  State<MediaFormatPicker> createState() => _MediaFormatPickerState();
}

class _MediaFormatPickerState extends State<MediaFormatPicker> {
  MediaFormat? _selected;
  MediaOutputKind _output = MediaOutputKind.video;

  List<MediaFormat> get _formats => widget.inspection.formats
      .where((format) => _output == MediaOutputKind.video
          ? format.hasVideo
          : format.hasAudio)
      .toList();

  @override
  void didUpdateWidget(covariant MediaFormatPicker oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.inspection != widget.inspection) _selected = null;
  }

  @override
  Widget build(BuildContext context) {
    final formats = _formats;
    final selected = formats.contains(_selected) ? _selected : null;

    return Scaffold(
      appBar: AppBar(title: const Text('Choose format')),
      body: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.all(16),
              child: SegmentedButton<MediaOutputKind>(
                segments: const [
                  ButtonSegment(value: MediaOutputKind.video,
                      label: Text('Video'), icon: Icon(Icons.videocam_outlined)),
                  ButtonSegment(value: MediaOutputKind.audio,
                      label: Text('Audio'), icon: Icon(Icons.headphones_outlined)),
                ],
                selected: {_output},
                onSelectionChanged: (values) => setState(() {
                  _output = values.first;
                  _selected = null;
                }),
              ),
            ),
            Expanded(
              child: formats.isEmpty
                  ? const Center(child: Text('No formats available'))
                  : ListView.separated(
                      padding: const EdgeInsets.symmetric(horizontal: 16),
                      itemCount: formats.length,
                      separatorBuilder: (_, __) => const SizedBox(height: 8),
                      itemBuilder: (context, index) {
                        final format = formats[index];
                        final isSelected = format == selected;
                        return Card(
                          child: ListTile(
                            leading: Icon(_output == MediaOutputKind.video
                                ? Icons.movie_outlined
                                : Icons.audiotrack_outlined),
                            title: Text(format.label),
                            subtitle: Text(_details(format)),
                            trailing: Icon(isSelected
                                ? Icons.check_circle : Icons.circle_outlined),
                            selected: isSelected,
                            onTap: () => setState(() => _selected = format),
                          ),
                        );
                      },
                    ),
            ),
            Padding(
              padding: const EdgeInsets.all(16),
              child: FilledButton.icon(
                onPressed: selected == null ? null : () => widget.onQueue(
                  MediaTaskRequest(
                    source: widget.inspection.source,
                    format: selected,
                    outputPath: widget.outputPath,
                  ),
                ),
                icon: const Icon(Icons.add_to_queue),
                label: const Text('Add to downloads'),
              ),
            ),
          ],
        ),
      ),
    );
  }

  String _details(MediaFormat format) {
    final parts = <String>[];
    if (format.width != null && format.height != null) {
      parts.add('${format.width} × ${format.height}');
    }
    if (format.bitrateKbps != null) parts.add('${format.bitrateKbps} kb/s');
    if (format.fileSizeBytes != null) parts.add(_formatBytes(format.fileSizeBytes!));
    if (format.requiresMerge) parts.add('Audio/video merge');
    if (parts.isEmpty) parts.add(format.container.name.toUpperCase());
    return parts.join(' • ');
  }

  String _formatBytes(int bytes) {
    const units = ['B', 'KB', 'MB', 'GB'];
    var size = bytes.toDouble();
    var unit = 0;
    while (size >= 1024 && unit < units.length - 1) {
      size /= 1024;
      unit++;
    }
    return '${size.toStringAsFixed(unit == 0 ? 0 : 1)} ${units[unit]}';
  }
}
