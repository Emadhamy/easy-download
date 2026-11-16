import 'package:easy_download/main.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('shows a readable error and retry action when Rust startup fails',
      (tester) async {
    await tester.pumpWidget(
      EasyDownloadApp(
        initializeRust: () async {
          throw StateError('test native library failure');
        },
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Easy Download could not start'), findsOneWidget);
    expect(find.textContaining('test native library failure'), findsOneWidget);
    expect(find.text('Retry engine startup'), findsOneWidget);
  });
}
