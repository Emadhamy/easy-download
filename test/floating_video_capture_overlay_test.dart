import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:easy_download/src/media/floating_video_capture_overlay.dart';

void main() {
  testWidgets('shows the glass video action above its child', (tester) async {
    var captured = 0;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 420,
            height: 260,
            child: FloatingVideoCaptureOverlay(
              onCapture: () => captured++,
              child: const ColoredBox(color: Colors.black),
            ),
          ),
        ),
      ),
    );

    expect(find.text('Download video'), findsOneWidget);
    expect(find.byIcon(Icons.movie_creation_outlined), findsOneWidget);
    await tester.tap(find.byTooltip('Download video'));
    expect(captured, 1);
  });

  testWidgets('can be hidden and dismissed by its host', (tester) async {
    var dismissed = 0;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 420,
            height: 260,
            child: FloatingVideoCaptureOverlay(
              onCapture: () {},
              onDismiss: () => dismissed++,
              child: const ColoredBox(color: Colors.black),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.byTooltip('Hide'));
    expect(dismissed, 1);

    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 420,
            height: 260,
            child: FloatingVideoCaptureOverlay(
              visible: false,
              onCapture: _noop,
              child: ColoredBox(color: Colors.black),
            ),
          ),
        ),
      ),
    );
    expect(find.text('Download video'), findsNothing);
  });
}

void _noop() {}
