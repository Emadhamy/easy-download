import 'package:flutter/material.dart';

void main() {
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

class _LinkDraft {
  const _LinkDraft({required this.url, required this.fileName});

  final Uri url;
  final String fileName;
}

class DownloadHomePage extends StatefulWidget {
  const DownloadHomePage({super.key});

  @override
  State<DownloadHomePage> createState() => _DownloadHomePageState();
}

class _DownloadHomePageState extends State<DownloadHomePage> {
  final List<_LinkDraft> _links = [];

  Future<void> _addLink() async {
    final controller = TextEditingController();
    String? errorText;

    final result = await showDialog<Uri>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setDialogState) => AlertDialog(
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
              onSubmitted: (_) {
                final uri = _validateUrl(controller.text);
                if (uri == null) {
                  setDialogState(() => errorText =
                      'Enter a valid link starting with http:// or https://');
                } else {
                  Navigator.of(dialogContext).pop(uri);
                }
              },
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
              onPressed: () {
                final uri = _validateUrl(controller.text);
                if (uri == null) {
                  setDialogState(() => errorText =
                      'Enter a valid link starting with http:// or https://');
                  return;
                }
                Navigator.of(dialogContext).pop(uri);
              },
              icon: const Icon(Icons.link_rounded),
              label: const Text('Add link'),
            ),
          ],
        ),
      ),
    );

    controller.dispose();
    if (!mounted || result == null) return;

    final segment = result.pathSegments.isEmpty
        ? ''
        : Uri.decodeComponent(result.pathSegments.last);
    final fileName = segment.isEmpty ? result.host : segment;

    setState(() {
      _links.insert(0, _LinkDraft(url: result, fileName: fileName));
    });

    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(
        content: Text('Link added to this session. Downloading is not connected yet.'),
      ),
    );
  }

  Uri? _validateUrl(String input) {
    final uri = Uri.tryParse(input.trim());
    if (uri == null ||
        !uri.hasAuthority ||
        uri.host.isEmpty ||
        !(uri.scheme == 'http' || uri.scheme == 'https')) {
      return null;
    }
    return uri;
  }

  @override
  Widget build(BuildContext context) {
    const violet = Color(0xFF8B5CF6);
    const muted = Color(0xFF9A9AAF);
    final activeCount = 0;
    final completedCount = 0;

    return Scaffold(
      appBar: AppBar(
        title: const Row(
          children: [
            Icon(Icons.downloading_rounded, color: violet, size: 29),
            SizedBox(width: 10),
            Text('Easy Download',
                style: TextStyle(fontWeight: FontWeight.w700)),
          ],
        ),
        actions: [
          IconButton(
            tooltip: 'Settings (coming soon)',
            onPressed: () => _showNotice(context),
            icon: const Icon(Icons.settings_outlined),
          ),
          const SizedBox(width: 8),
        ],
      ),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _addLink,
        icon: const Icon(Icons.add_link_rounded),
        label: const Text('New download'),
      ),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.fromLTRB(20, 12, 20, 100),
          children: [
            const Text(
              'Your downloads,\nall in one place.',
              style: TextStyle(
                  fontSize: 28, height: 1.15, fontWeight: FontWeight.w800),
            ),
            const SizedBox(height: 8),
            const Text(
              'A clean home for files, videos, and torrents.',
              style: TextStyle(color: muted, fontSize: 14),
            ),
            const SizedBox(height: 24),
            Row(
              children: [
                Expanded(
                  child: _StatCard(
                    label: 'Active',
                    value: '$activeCount',
                    icon: Icons.downloading_rounded,
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: _StatCard(
                    label: 'Completed',
                    value: '$completedCount',
                    icon: Icons.check_circle_outline_rounded,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 28),
            const Row(
              children: [
                Text('Downloads',
                    style:
                        TextStyle(fontSize: 19, fontWeight: FontWeight.w700)),
                Spacer(),
                Text('This session',
                    style: TextStyle(color: muted, fontSize: 13)),
              ],
            ),
            const SizedBox(height: 12),
            if (_links.isEmpty)
              const _EmptyState()
            else
              ..._links.map((link) => Padding(
                    padding: const EdgeInsets.only(bottom: 10),
                    child: Card(
                      child: ListTile(
                        leading: const CircleAvatar(
                          backgroundColor: Color(0x268B5CF6),
                          child: Icon(Icons.link_rounded, color: violet),
                        ),
                        title: Text(link.fileName,
                            maxLines: 1, overflow: TextOverflow.ellipsis),
                        subtitle: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            const SizedBox(height: 4),
                            Text(
                              link.url.toString(),
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                            ),
                            const SizedBox(height: 6),
                            const Text(
                              'Ready · download engine not connected',
                              style: TextStyle(color: muted, fontSize: 12),
                            ),
                          ],
                        ),
                      ),
                    ),
                  )),
            const SizedBox(height: 24),
            const Card(
              child: Padding(
                padding: EdgeInsets.all(16),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Icon(Icons.construction_rounded, color: violet),
                    SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text('Starter build',
                              style: TextStyle(fontWeight: FontWeight.w700)),
                          SizedBox(height: 5),
                          Text(
                            'Links added here are kept in memory for this session only. Real downloading, persistence, video extraction, and BitTorrent are not connected yet.',
                            style: TextStyle(
                                color: muted, height: 1.4, fontSize: 13),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
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
