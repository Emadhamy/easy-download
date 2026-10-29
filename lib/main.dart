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

class DownloadHomePage extends StatelessWidget {
  const DownloadHomePage({super.key});

  @override
  Widget build(BuildContext context) {
    const violet = Color(0xFF8B5CF6);
    const muted = Color(0xFF9A9AAF);

    return Scaffold(
      appBar: AppBar(
        title: const Row(
          children: [
            Icon(Icons.downloading_rounded, color: violet, size: 29),
            SizedBox(width: 10),
            Text('Easy Download', style: TextStyle(fontWeight: FontWeight.w700)),
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
        onPressed: () => _showNotice(context),
        icon: const Icon(Icons.add_link_rounded),
        label: const Text('New download'),
      ),
      body: SafeArea(
        child: ListView(
          padding: const EdgeInsets.fromLTRB(20, 12, 20, 100),
          children: [
            const Text(
              'Your downloads,\nall in one place.',
              style: TextStyle(fontSize: 28, height: 1.15, fontWeight: FontWeight.w800),
            ),
            const SizedBox(height: 8),
            const Text(
              'A clean home for files, videos, and torrents.',
              style: TextStyle(color: muted, fontSize: 14),
            ),
            const SizedBox(height: 24),
            const Row(
              children: [
                Expanded(child: _StatCard(label: 'Active', value: '0', icon: Icons.downloading_rounded)),
                SizedBox(width: 12),
                Expanded(child: _StatCard(label: 'Completed', value: '0', icon: Icons.check_circle_outline_rounded)),
              ],
            ),
            const SizedBox(height: 28),
            const Row(
              children: [
                Text('Downloads', style: TextStyle(fontSize: 19, fontWeight: FontWeight.w700)),
                Spacer(),
                Text('All files', style: TextStyle(color: muted, fontSize: 13)),
              ],
            ),
            const SizedBox(height: 12),
            const _EmptyState(),
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
                          Text('Starter build', style: TextStyle(fontWeight: FontWeight.w700)),
                          SizedBox(height: 5),
                          Text(
                            'This is the first UI scaffold. Downloading, video extraction, and BitTorrent are not connected yet.',
                            style: TextStyle(color: muted, height: 1.4, fontSize: 13),
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
      const SnackBar(content: Text('Download actions will be connected in a later step.')),
    );
  }
}

class _StatCard extends StatelessWidget {
  const _StatCard({required this.label, required this.value, required this.icon});

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
            Text(value, style: const TextStyle(fontSize: 26, fontWeight: FontWeight.w800)),
            const SizedBox(height: 3),
            Text(label, style: const TextStyle(color: Color(0xFF9A9AAF), fontSize: 13)),
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
                color: const Color(0xFF8B5CF6).withOpacity(0.14),
                shape: BoxShape.circle,
              ),
              child: const Icon(Icons.download_for_offline_outlined, size: 32, color: Color(0xFF8B5CF6)),
            ),
            const SizedBox(height: 14),
            const Text('Nothing downloading yet', style: TextStyle(fontSize: 16, fontWeight: FontWeight.w700)),
            const SizedBox(height: 6),
            const Text(
              'Add a direct link to start building your download list.',
              textAlign: TextAlign.center,
              style: TextStyle(color: Color(0xFF9A9AAF), height: 1.4),
            ),
          ],
        ),
      ),
    );
  }
}
