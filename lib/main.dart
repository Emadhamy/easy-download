import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
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

class _LinkDraft {
  _LinkDraft({required this.url, required this.fileName});
  final Uri url;
  final String fileName;
  bool isDownloading = false;
  String? savedPath;
}

class DownloadHomePage extends StatefulWidget {
  const DownloadHomePage({super.key});

  @override
  State<DownloadHomePage> createState() => _DownloadHomePageState();
}

class _DownloadHomePageState extends State<DownloadHomePage> {
  final List<_LinkDraft> _links = [];

  Future<void> _download(_LinkDraft link) async {
    if (link.isDownloading || link.savedPath != null) return;
    setState(() => link.isDownloading = true);
    try {
      final directory = await getApplicationDocumentsDirectory();
      final safeName = link.fileName.replaceAll('/', '_');
      final destination = directory.path + '/' + safeName;
      await rust_api.downloadFile(inputUrl: link.url.toString(), destinationPath: destination);
      if (!mounted) return;
      setState(() { link.savedPath = destination; link.isDownloading = false; });
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('Saved to app storage: $safeName')));
    } catch (_) {
      if (!mounted) return;
      setState(() => link.isDownloading = false);
      ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('Download failed. Check the link and connection.')));
    }
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


  @override
  Widget build(BuildContext context) {
    const violet = Color(0xFF8B5CF6);
    const muted = Color(0xFF9A9AAF);
    const panel = Color(0xFF151525);
    final queuedCount = _links.length;

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
                const Expanded(
                  child: _StatCard(
                    label: 'Downloaded',
                    value: '0',
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
                Text(
                  queuedCount == 0 ? 'Nothing added' : '$queuedCount saved this session',
                  style: const TextStyle(color: muted, fontSize: 12),
                ),
              ],
            ),
            const SizedBox(height: 12),
            if (_links.isEmpty)
              const _EmptyState()
            else
              ..._links.map((link) => Padding(
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
                          child: const Icon(Icons.insert_drive_file_outlined,
                              color: violet),
                        ),
                        title: Text(link.fileName,
                            maxLines: 1, overflow: TextOverflow.ellipsis,
                            style: const TextStyle(fontWeight: FontWeight.w700)),
                        subtitle: Padding(
                          padding: const EdgeInsets.only(top: 5),
                          child: Text(
                            link.url.toString(),
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(color: muted, fontSize: 12),
                          ),
                        ),
                        trailing: link.savedPath != null
                            ? const Icon(Icons.check_circle_rounded, color: Color(0xFF4ADE80))
                            : link.isDownloading
                                ? const SizedBox(width: 24, height: 24, child: CircularProgressIndicator(strokeWidth: 2))
                                : IconButton(tooltip: 'Download', onPressed: () => _download(link), icon: const Icon(Icons.download_rounded, color: violet)),
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
                      'Downloads save inside app storage. History is session-only; progress controls are coming next.',
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
