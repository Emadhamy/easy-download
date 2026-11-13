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
    return ThemeData(
      useMaterial3: true,
      brightness: Brightness.dark,
      scaffoldBackgroundColor: EasyDownloadPalette.background,
      colorScheme: ColorScheme.fromSeed(
        seedColor: EasyDownloadPalette.accent,
        brightness: Brightness.dark,
        surface: EasyDownloadPalette.surface,
        primary: EasyDownloadPalette.accent,
        onPrimary: const Color(0xFF17110D),
        secondary: EasyDownloadPalette.accentSoft,
        onSurface: EasyDownloadPalette.foreground,
        error: EasyDownloadPalette.danger,
      ),
      appBarTheme: const AppBarTheme(
        backgroundColor: EasyDownloadPalette.background,
        foregroundColor: EasyDownloadPalette.foreground,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
      ),
      floatingActionButtonTheme: const FloatingActionButtonThemeData(
        backgroundColor: EasyDownloadPalette.accent,
        foregroundColor: Color(0xFF17110D),
      ),
      snackBarTheme: const SnackBarThemeData(
        backgroundColor: EasyDownloadPalette.elevatedSurface,
        contentTextStyle: TextStyle(color: EasyDownloadPalette.foreground),
      ),
    );
  }
}
