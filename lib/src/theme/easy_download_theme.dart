import 'package:flutter/material.dart';

/// Shared visual identity for Easy Download across desktop and Android.
abstract final class EasyDownloadPalette {
  static const background = Color(0xFF111111);
  static const surface = Color(0xFF202020);
  static const elevatedSurface = Color(0xFF2B2B2B);
  static const accent = Color(0xFFFF6A00);
  static const accentBright = Color(0xFFFF7A1A);
  static const accentSoft = Color(0xFFFFB36B);
  static const foreground = Color(0xFFF7F4F1);
  static const muted = Color(0xFFAAA39D);
  static const border = Color(0xFF554033);
  static const success = Color(0xFF43C98B);
  static const danger = Color(0xFFFF5A52);
}

abstract final class EasyDownloadTheme {
  static ThemeData get dark {
    const palette = EasyDownloadPalette;
    return ThemeData(
      useMaterial3: true,
      brightness: Brightness.dark,
      scaffoldBackgroundColor: palette.background,
      colorScheme: ColorScheme.fromSeed(
        seedColor: palette.accent,
        brightness: Brightness.dark,
        surface: palette.surface,
        primary: palette.accent,
        onPrimary: const Color(0xFF17110D),
        secondary: palette.accentSoft,
        onSurface: palette.foreground,
        error: palette.danger,
      ),
      appBarTheme: const AppBarTheme(
        backgroundColor: palette.background,
        foregroundColor: palette.foreground,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
      ),
      floatingActionButtonTheme: const FloatingActionButtonThemeData(
        backgroundColor: palette.accent,
        foregroundColor: Color(0xFF17110D),
      ),
      snackBarTheme: const SnackBarThemeData(
        backgroundColor: palette.elevatedSurface,
        contentTextStyle: TextStyle(color: palette.foreground),
      ),
    );
  }
}
