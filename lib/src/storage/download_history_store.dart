import 'dart:convert';
import 'dart:io';

/// Crash-resilient local persistence for the download queue and history.
///
/// Writes are serialized and committed through a staging file. The previous
/// valid snapshot is retained as a backup so startup can recover if the app is
/// interrupted while replacing the primary file.
class DownloadHistoryStore {
  DownloadHistoryStore(this.file);

  final File file;
  Future<void> _pendingWrite = Future<void>.value();

  File get _temporaryFile => File('${file.path}.tmp');
  File get _backupFile => File('${file.path}.bak');

  Future<List<Map<String, dynamic>>?> load() async {
    for (final candidate in [file, _backupFile]) {
      if (!await candidate.exists()) continue;
      try {
        final decoded = jsonDecode(await candidate.readAsString());
        if (decoded is! List) continue;
        final entries = <Map<String, dynamic>>[];
        for (final entry in decoded) {
          if (entry is Map) {
            entries.add(Map<String, dynamic>.from(entry));
          }
        }
        return entries;
      } on FormatException {
        // Try the backup when the primary snapshot is corrupt.
      } on FileSystemException {
        // A missing or unreadable snapshot must not prevent app startup.
      } on TypeError {
        // Ignore malformed entries and continue with the next snapshot.
      }
    }
    return null;
  }

  Future<void> save(List<Map<String, dynamic>> entries) {
    final snapshot = jsonEncode(entries);
    final write = _pendingWrite
        .catchError((Object _) {})
        .then((_) => _writeSnapshot(snapshot));
    _pendingWrite = write;
    return write;
  }

  Future<void> _writeSnapshot(String snapshot) async {
    await file.parent.create(recursive: true);
    await _temporaryFile.writeAsString(snapshot, flush: true);

    var movedPrimaryToBackup = false;
    try {
      if (await _backupFile.exists()) {
        await _backupFile.delete();
      }
      if (await file.exists()) {
        await file.rename(_backupFile.path);
        movedPrimaryToBackup = true;
      }
      await _temporaryFile.rename(file.path);
    } catch (_) {
      if (movedPrimaryToBackup &&
          !await file.exists() &&
          await _backupFile.exists()) {
        await _backupFile.rename(file.path);
      }
      rethrow;
    } finally {
      if (await _temporaryFile.exists()) {
        await _temporaryFile.delete();
      }
    }
  }
}
