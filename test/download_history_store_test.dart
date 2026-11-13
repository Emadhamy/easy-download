import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:easy_download/src/storage/download_history_store.dart';

void main() {
  late Directory directory;
  late File file;
  late DownloadHistoryStore store;

  setUp(() async {
    directory = await Directory.systemTemp.createTemp('easy-download-history-');
    file = File('${directory.path}/history.json');
    store = DownloadHistoryStore(file);
  });

  tearDown(() async {
    if (await directory.exists()) {
      await directory.delete(recursive: true);
    }
  });

  test('returns null when no snapshot exists', () async {
    expect(await store.load(), isNull);
  });

  test('writes and restores a download snapshot', () async {
    final entries = [
      {'url': 'https://example.com/file.zip', 'receivedBytes': 128},
    ];

    await store.save(entries);

    expect(await store.load(), entries);
  });

  test('recovers the previous valid snapshot when the primary is corrupt',
      () async {
    final previous = [
      {'url': 'https://example.com/previous.zip'},
    ];
    await store.save(previous);
    await store.save([
      {'url': 'https://example.com/current.zip'},
    ]);

    await file.writeAsString('{invalid json');

    expect(await store.load(), previous);
  });

  test('serializes overlapping writes and keeps the latest snapshot',
      () async {
    final first = store.save([
      {'url': 'https://example.com/first.zip'},
    ]);
    final second = store.save([
      {'url': 'https://example.com/second.zip'},
    ]);
    final third = store.save([
      {'url': 'https://example.com/third.zip'},
    ]);

    await Future.wait([first, second, third]);

    expect(
      (await store.load())?.single['url'],
      'https://example.com/third.zip',
    );
  });

  test('ignores malformed entries but restores valid map entries', () async {
    await file.parent.create(recursive: true);
    await file.writeAsString(jsonEncode([
      {'url': 'https://example.com/valid.zip'},
      'not-a-map',
      42,
    ]));

    expect((await store.load())?.length, 1);
  });
}
