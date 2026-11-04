import 'package:flutter/material.dart';

import 'media_models.dart';

/// Displays tool lifecycle state. Real update/rollback operations are supplied
/// by a ManagedToolController implementation in a later integration step.
class ManagedToolsScreen extends StatefulWidget {
  const ManagedToolsScreen({
    super.key,
    required this.controller,
  });

  final ManagedToolController controller;

  @override
  State<ManagedToolsScreen> createState() => _ManagedToolsScreenState();
}

class _ManagedToolsScreenState extends State<ManagedToolsScreen> {
  late Future<List<ManagedToolStatus>> _statuses;

  @override
  void initState() {
    super.initState();
    _reload();
  }

  void _reload() {
    _statuses = Future.wait([
      widget.controller.status(ManagedTool.ytDlp),
      widget.controller.status(ManagedTool.ffmpeg),
    ]);
  }

  Future<void> _refresh() async {
    setState(_reload);
    await _statuses;
  }

  Future<void> _update(ManagedTool tool) async {
    final result = await widget.controller.update(tool);
    if (!mounted) return;
    setState(_reload);
    final message = result.health == ToolHealth.ready
        ? '${_name(tool)} is ready (${result.installedVersion ?? 'version unknown'}).'
        : result.lastError ?? 'Update did not complete.';
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(message)),
    );
  }

  Future<void> _rollback(ManagedTool tool) async {
    final succeeded = await widget.controller.rollback(tool);
    if (!mounted) return;
    setState(_reload);
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(succeeded
          ? '${_name(tool)} rollback completed.'
          : 'No previous version could be restored.')),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Media tools'),
        actions: [
          IconButton(
            tooltip: 'Refresh status',
            onPressed: _refresh,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: FutureBuilder<List<ManagedToolStatus>>(
        future: _statuses,
        builder: (context, snapshot) {
          if (snapshot.connectionState == ConnectionState.waiting) {
            return const Center(child: CircularProgressIndicator());
          }
          if (snapshot.hasError) {
            return Center(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Text('Could not read tool status: ${snapshot.error}'),
              ),
            );
          }
          final statuses = snapshot.data ?? const <ManagedToolStatus>[];
          if (statuses.isEmpty) {
            return const Center(child: Text('No tool status available.'));
          }
          return ListView(
            padding: const EdgeInsets.all(16),
            children: [
              const Text(
                'Video and audio components',
                style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
              ),
              const SizedBox(height: 8),
              const Text(
                'Each component has its own version and lifecycle. '
                'This screen uses the controller provided by the app.',
              ),
              const SizedBox(height: 16),
              for (final status in statuses) ...[
                _ToolCard(
                  status: status,
                  onUpdate: () => _update(status.tool),
                  onRollback: () => _rollback(status.tool),
                ),
                const SizedBox(height: 12),
              ],
            ],
          );
        },
      ),
    );
  }

  String _name(ManagedTool tool) =>
      tool == ManagedTool.ytDlp ? 'yt-dlp' : 'FFmpeg';
}

class _ToolCard extends StatelessWidget {
  const _ToolCard({
    required this.status,
    required this.onUpdate,
    required this.onRollback,
  });

  final ManagedToolStatus status;
  final VoidCallback onUpdate;
  final VoidCallback onRollback;

  @override
  Widget build(BuildContext context) {
    final toolName = status.tool == ManagedTool.ytDlp ? 'yt-dlp' : 'FFmpeg';
    final version = status.installedVersion ?? 'Not installed';
    final available = status.availableVersion == null
        ? ''
        : 'Available: ${status.availableVersion}';

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(status.tool == ManagedTool.ytDlp
                    ? Icons.video_library_outlined
                    : Icons.build_circle_outlined),
                const SizedBox(width: 12),
                Expanded(
                  child: Text(toolName,
                      style: Theme.of(context).textTheme.titleLarge),
                ),
                Chip(label: Text(_healthLabel(status.health))),
              ],
            ),
            const SizedBox(height: 8),
            Text('Installed: $version'),
            if (available.isNotEmpty) Text(available),
            if (status.platformArtifact != null)
              Text('Package: ${status.platformArtifact}'),
            if (status.lastError != null) ...[
              const SizedBox(height: 8),
              Text(status.lastError!,
                  style: TextStyle(color: Theme.of(context).colorScheme.error)),
            ],
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                FilledButton.tonalIcon(
                  onPressed: status.health == ToolHealth.updating
                      ? null
                      : onUpdate,
                  icon: const Icon(Icons.system_update_alt),
                  label: Text(status.health == ToolHealth.updateAvailable
                      ? 'Update' : 'Install / check'),
                ),
                OutlinedButton.icon(
                  onPressed: status.installedVersion == null
                      ? null
                      : onRollback,
                  icon: const Icon(Icons.history),
                  label: const Text('Rollback'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  String _healthLabel(ToolHealth health) {
    switch (health) {
      case ToolHealth.unavailable:
        return 'Unavailable';
      case ToolHealth.notInstalled:
        return 'Not installed';
      case ToolHealth.ready:
        return 'Ready';
      case ToolHealth.updateAvailable:
        return 'Update available';
      case ToolHealth.updating:
        return 'Updating';
      case ToolHealth.failed:
        return 'Failed';
    }
  }
}
