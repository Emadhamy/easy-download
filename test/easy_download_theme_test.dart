import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:easy_download/src/theme/easy_download_theme.dart';

void main() {
  group('Easy Download brand theme', () {
    test('uses the orange accent and charcoal surfaces', () {
      final theme = EasyDownloadTheme.dark;

      expect(theme.brightness, Brightness.dark);
      expect(theme.colorScheme.primary, EasyDownloadPalette.accent);
      expect(theme.scaffoldBackgroundColor, EasyDownloadPalette.background);
      expect(theme.colorScheme.surface, EasyDownloadPalette.surface);
      expect(
        theme.floatingActionButtonTheme.backgroundColor,
        EasyDownloadPalette.accent,
      );
    });

    test('keeps the accent palette independent from the old violet theme',
        () {
      expect(EasyDownloadPalette.accent, const Color(0xFFFF6A00));
      expect(EasyDownloadPalette.accent, isNot(const Color(0xFF8B5CF6)));
    });
  });
}
