import 'dart:ui';

import 'package:flutter/material.dart';

import '../theme/easy_download_theme.dart';

/// A draggable glass-style video capture action that can sit above a video
/// surface. A host video/browser integration supplies the download callback.
class FloatingVideoCaptureOverlay extends StatefulWidget {
  const FloatingVideoCaptureOverlay({
    super.key,
    required this.child,
    required this.onCapture,
    this.visible = true,
    this.onDismiss,
    this.label = 'Download video',
  });

  final Widget child;
  final VoidCallback onCapture;
  final bool visible;
  final VoidCallback? onDismiss;
  final String label;

  @override
  State<FloatingVideoCaptureOverlay> createState() =>
      _FloatingVideoCaptureOverlayState();
}

class _FloatingVideoCaptureOverlayState
    extends State<FloatingVideoCaptureOverlay> {
  static const _overlayWidth = 188.0;
  static const _overlayHeight = 58.0;
  double _right = 16;
  double _top = 16;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final maxRight = (constraints.maxWidth - _overlayWidth - 8)
            .clamp(8.0, double.infinity)
            .toDouble();
        final maxTop = (constraints.maxHeight - _overlayHeight - 8)
            .clamp(8.0, double.infinity)
            .toDouble();
        _right = _right.clamp(8.0, maxRight).toDouble();
        _top = _top.clamp(8.0, maxTop).toDouble();

        return Stack(
          fit: StackFit.expand,
          children: [
            widget.child,
            if (widget.visible)
              Positioned(
                right: _right,
                top: _top,
                width: _overlayWidth,
                height: _overlayHeight,
                child: GestureDetector(
                  onPanUpdate: (details) {
                    setState(() {
                      _right = (_right - details.delta.dx)
                          .clamp(8.0, maxRight)
                          .toDouble();
                      _top = (_top + details.delta.dy)
                          .clamp(8.0, maxTop)
                          .toDouble();
                    });
                  },
                  child: ClipRRect(
                    borderRadius: BorderRadius.circular(22),
                    child: BackdropFilter(
                      filter: ImageFilter.blur(sigmaX: 16, sigmaY: 16),
                      child: DecoratedBox(
                        decoration: BoxDecoration(
                          color: EasyDownloadPalette.surface
                              .withValues(alpha: 0.78),
                          borderRadius: BorderRadius.circular(22),
                          border: Border.all(
                            color: EasyDownloadPalette.accent
                                .withValues(alpha: 0.9),
                            width: 1.5,
                          ),
                          boxShadow: [
                            BoxShadow(
                              color: EasyDownloadPalette.accent
                                  .withValues(alpha: 0.18),
                              blurRadius: 22,
                              spreadRadius: 1,
                            ),
                          ],
                        ),
                        child: Row(
                          children: [
                            const SizedBox(width: 10),
                            const Icon(
                              Icons.movie_creation_outlined,
                              color: EasyDownloadPalette.accent,
                              size: 25,
                            ),
                            const SizedBox(width: 8),
                            Expanded(
                              child: Text(
                                widget.label,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: const TextStyle(
                                  color: EasyDownloadPalette.foreground,
                                  fontSize: 12,
                                  fontWeight: FontWeight.w700,
                                ),
                              ),
                            ),
                            IconButton(
                              tooltip: widget.label,
                              visualDensity: VisualDensity.compact,
                              onPressed: widget.onCapture,
                              style: IconButton.styleFrom(
                                backgroundColor: EasyDownloadPalette.accent,
                                foregroundColor: const Color(0xFF17110D),
                              ),
                              icon: const Icon(Icons.download_rounded, size: 21),
                            ),
                            if (widget.onDismiss != null)
                              IconButton(
                                tooltip: 'Hide',
                                visualDensity: VisualDensity.compact,
                                onPressed: widget.onDismiss,
                                icon: const Icon(
                                  Icons.close_rounded,
                                  size: 17,
                                  color: EasyDownloadPalette.muted,
                                ),
                              ),
                            const SizedBox(width: 5),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ),
          ],
        );
      },
    );
  }
}
