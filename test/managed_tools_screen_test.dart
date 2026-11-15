import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:easy_download/src/media/managed_tools_screen.dart';
import 'package:easy_download/src/media/media_models.dart';

void main() {
  group('ManagedToolsScreen', () {
    testWidgets('updates FFmpeg without changing yt-dlp status', (tester) async {
      final controller = _FakeManagedToolController({
        ManagedTool.ytDlp: const ManagedToolStatus(
          tool: ManagedTool.ytDlp,
          health: ToolHealth.ready,
          installedVersion: '2026.10.01',
        ),
        ManagedTool.ffmpeg: const ManagedToolStatus(
          tool: ManagedTool.ffmpeg,
          health: ToolHealth.notInstalled,
        ),
      });

      await tester.pumpWidget(_screen(controller));
      await tester.pumpAndSettle();

      final ffmpegCard = find.ancestor(
        of: find.text('FFmpeg'),
        matching: find.byType(Card),
      ).first;
      await tester.tap(find.descendant(
        of: ffmpegCard,
        matching: find.widgetWithText(FilledButton, 'Install / check'),
      ));
      await tester.pumpAndSettle();

      expect(controller.updatedTools, [ManagedTool.ffmpeg]);
      expect(find.text('Installed: 7.1.1'), findsOneWidget);
      expect(find.text('Installed: 2026.10.01'), findsOneWidget);
      expect(controller.statuses[ManagedTool.ytDlp]!.installedVersion,
          '2026.10.01');
    });

    testWidgets('rolls back only the selected tool', (tester) async {
      final controller = _FakeManagedToolController({
        ManagedTool.ytDlp: const ManagedToolStatus(
          tool: ManagedTool.ytDlp,
          health: ToolHealth.ready,
          installedVersion: '2026.10.01',
        ),
        ManagedTool.ffmpeg: const ManagedToolStatus(
          tool: ManagedTool.ffmpeg,
          health: ToolHealth.ready,
          installedVersion: '7.1.1',
        ),
      });

      await tester.pumpWidget(_screen(controller));
      await tester.pumpAndSettle();

      final ffmpegCard = find.ancestor(
        of: find.text('FFmpeg'),
        matching: find.byType(Card),
      ).first;
      await tester.tap(find.descendant(
        of: ffmpegCard,
        matching: find.widgetWithText(OutlinedButton, 'Rollback'),
      ));
      await tester.pumpAndSettle();

      expect(controller.rolledBackTools, [ManagedTool.ffmpeg]);
      expect(controller.rolledBackTools, isNot(contains(ManagedTool.ytDlp)));
    });

    testWidgets('shows an update failure without marking the tool ready',
        (tester) async {
      final controller = _FakeManagedToolController({
        ManagedTool.ytDlp: const ManagedToolStatus(
          tool: ManagedTool.ytDlp,
          health: ToolHealth.ready,
          installedVersion: '2026.10.01',
        ),
        ManagedTool.ffmpeg: const ManagedToolStatus(
          tool: ManagedTool.ffmpeg,
          health: ToolHealth.notInstalled,
        ),
      }, failFfmpegUpdate: true);

      await tester.pumpWidget(_screen(controller));
      await tester.pumpAndSettle();

      final ffmpegCard = find.ancestor(
        of: find.text('FFmpeg'),
        matching: find.byType(Card),
      ).first;
      await tester.tap(find.descendant(
        of: ffmpegCard,
        matching: find.widgetWithText(FilledButton, 'Install / check'),
      ));
      await tester.pumpAndSettle();

      expect(find.text('Test update failure'), findsAtLeastNWidgets(1));
      expect(controller.statuses[ManagedTool.ffmpeg]!.health, ToolHealth.failed);
      expect(controller.statuses[ManagedTool.ytDlp]!.health, ToolHealth.ready);
    });
  });
}

Widget _screen(ManagedToolController controller) => MaterialApp(
      home: ManagedToolsScreen(controller: controller),
    );

class _FakeManagedToolController implements ManagedToolController {
  _FakeManagedToolController(
    this.statuses, {
    this.failFfmpegUpdate = false,
  });

  final Map<ManagedTool, ManagedToolStatus> statuses;
  final bool failFfmpegUpdate;
  final List<ManagedTool> updatedTools = [];
  final List<ManagedTool> rolledBackTools = [];

  @override
  Future<ManagedToolStatus> status(ManagedTool tool) async => statuses[tool]!;

  @override
  Future<ManagedToolStatus> update(ManagedTool tool) async {
    updatedTools.add(tool);
    if (tool == ManagedTool.ffmpeg && failFfmpegUpdate) {
      final failed = ManagedToolStatus(
        tool: tool,
        health: ToolHealth.failed,
        lastError: 'Test update failure',
      );
      statuses[tool] = failed;
      return failed;
    }
    final ready = ManagedToolStatus(
      tool: tool,
      health: ToolHealth.ready,
      installedVersion: tool == ManagedTool.ffmpeg ? '7.1.1' : '2026.10.01',
    );
    statuses[tool] = ready;
    return ready;
  }

  @override
  Future<bool> rollback(ManagedTool tool) async {
    rolledBackTools.add(tool);
    return true;
  }
}
