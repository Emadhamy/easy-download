import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:easy_download/src/media/ffmpeg_manager_card.dart';

void main() {
  group('FfmpegManagerCard direct interaction', () {
    testWidgets('routes manager actions to FFmpeg callbacks only',
        (tester) async {
      final actions = <String>[];
      const ytDlpVersion = '2026.10.01';

      await tester.pumpWidget(_host(
        onChooseExecutable: () => actions.add('choose'),
        onCheckUpdates: () => actions.add('check'),
        onInstallLatest: () => actions.add('install'),
        onRollback: () => actions.add('rollback'),
        onVerify: () => actions.add('verify'),
        onChooseVideo: () => actions.add('video'),
        onConvert: () => actions.add('convert'),
      ));

      await tester.tap(find.text('Change FFmpeg'));
      await tester.tap(find.text('Check updates'));
      await tester.tap(find.text('Install / Update'));
      await tester.tap(find.text('Rollback'));
      await tester.tap(find.text('Verify'));
      await tester.tap(find.text('Choose video file'));
      await tester.tap(find.text('Convert to MP3'));
      await tester.pump();

      expect(actions, [
        'choose',
        'check',
        'install',
        'rollback',
        'verify',
        'video',
        'convert',
      ]);
      expect(ytDlpVersion, '2026.10.01');
    });

    testWidgets('selects an MP3 bitrate and reports the new value',
        (tester) async {
      String? selected;
      await tester.pumpWidget(_host(onBitrateChanged: (value) {
        selected = value;
      }));

      await tester.tap(find.byType(DropdownButtonFormField<String>));
      await tester.pumpAndSettle();
      await tester.tap(find.text('320k').last);
      await tester.pumpAndSettle();

      expect(selected, '320k');
    });

    testWidgets('routes pause, resume and cancel according to conversion state',
        (tester) async {
      final actions = <String>[];
      await tester.pumpWidget(_host(
        converting: true,
        onPause: () => actions.add('pause'),
        onCancel: () => actions.add('cancel'),
      ));

      await tester.tap(find.text('Pause'));
      await tester.tap(find.text('Cancel'));
      await tester.pump();

      expect(actions, ['pause', 'cancel']);

      await tester.pumpWidget(_host(
        converting: true,
        paused: true,
        onResume: () => actions.add('resume'),
      ));
      await tester.tap(find.text('Resume'));
      await tester.pump();

      expect(actions, ['pause', 'cancel', 'resume']);
    });

    testWidgets('disables installation and conversion actions when busy',
        (tester) async {
      await tester.pumpWidget(_host(
        checking: true,
        installing: true,
        converting: true,
      ));

      final check = tester.widget<OutlinedButton>(
        find.widgetWithText(OutlinedButton, 'Check updates'),
      );
      final install = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, 'Updating...'),
      );
      expect(check.onPressed, isNull);
      expect(install.onPressed, isNull);
      expect(find.text('00:12 / 00:30'), findsOneWidget);
    });

    testWidgets('keeps FFmpeg controls unavailable on Android', (tester) async {
      await tester.pumpWidget(_host(supported: false));

      expect(find.text('NOT AVAILABLE'), findsOneWidget);
      expect(find.textContaining('disabled on Android'), findsOneWidget);
      expect(
        tester.widget<OutlinedButton>(
          find.widgetWithText(OutlinedButton, 'Change FFmpeg'),
        ).onPressed,
        isNull,
      );
      expect(
        tester.widget<FilledButton>(
          find.widgetWithText(FilledButton, 'Install / Update'),
        ).onPressed,
        isNull,
      );
      expect(
        tester.widget<FilledButton>(
          find.widgetWithText(FilledButton, 'Convert to MP3'),
        ).onPressed,
        isNull,
      );
      expect(find.byType(SwitchListTile), findsNothing);
    });
  });
}

Widget _host({
  bool supported = true,
  bool checking = false,
  bool installing = false,
  bool converting = false,
  bool paused = false,
  VoidCallback? onChooseExecutable,
  VoidCallback? onCheckUpdates,
  VoidCallback? onInstallLatest,
  VoidCallback? onRollback,
  VoidCallback? onVerify,
  VoidCallback? onChooseVideo,
  ValueChanged<String?>? onBitrateChanged,
  VoidCallback? onConvert,
  VoidCallback? onPause,
  VoidCallback? onResume,
  VoidCallback? onCancel,
}) {
  return MaterialApp(
    home: Scaffold(
      body: SingleChildScrollView(
        child: FfmpegManagerCard(
          supported: supported,
          installedVersion: '7.1.1',
          latestVersion: '7.2.0',
          lastCheckedAt: '2026-11-15T09:00:00',
          executablePath: '/tools/ffmpeg',
          status: 'Ready for testing',
          checking: checking,
          installing: installing,
          receivedBytes: 512,
          totalBytes: 1024,
          converting: converting,
          paused: paused,
          inputPath: '/videos/sample.mp4',
          bitrate: '192k',
          elapsedMillis: 12000,
          totalMillis: 30000,
          autoCheckEnabled: true,
          history: const ['Installed FFmpeg 7.1.1'],
          onChooseExecutable: onChooseExecutable ?? () {},
          onCheckUpdates: onCheckUpdates ?? () {},
          onInstallLatest: onInstallLatest ?? () {},
          onRollback: onRollback ?? () {},
          onVerify: onVerify ?? () {},
          onChooseVideo: onChooseVideo ?? () {},
          onBitrateChanged: onBitrateChanged ?? (_) {},
          onConvert: onConvert ?? () {},
          onPause: onPause ?? () {},
          onResume: onResume ?? () {},
          onCancel: onCancel ?? () {},
          onAutoCheckChanged: (_) {},
        ),
      ),
    ),
  );
}
