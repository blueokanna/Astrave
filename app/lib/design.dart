import 'package:flutter/physics.dart';
import 'package:material_ui/material_ui.dart';

typedef LocalText = String Function(String english, String chinese);

abstract final class Space {
  static const small = 8.0;
  static const medium = 16.0;
  static const large = 24.0;
  static const section = 32.0;
}

ThemeData desktopTheme(Brightness brightness, ColorScheme? dynamicScheme) {
  final scheme =
      dynamicScheme ??
      ColorScheme.fromSeed(
        seedColor: const Color(0xff6651a8),
        brightness: brightness,
        dynamicSchemeVariant: DynamicSchemeVariant.expressive,
      );
  return ThemeData(
    useMaterial3: true,
    fontFamily: 'GoogleSans',
    fontFamilyFallback: const ['HarmonyOSSans'],
    colorScheme: scheme,
    scaffoldBackgroundColor: scheme.surface,
    cardTheme: CardThemeData(
      elevation: 0,
      margin: EdgeInsets.zero,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(24)),
    ),
    navigationRailTheme: NavigationRailThemeData(
      backgroundColor: scheme.surfaceContainerLow,
    ),
    elevatedButtonTheme: const ElevatedButtonThemeData(
      variant: StyleVariant.material3Expressive,
    ),
    iconButtonTheme: const IconButtonThemeData(
      variant: StyleVariant.material3Expressive,
    ),
    inputDecorationTheme: const InputDecorationThemeData(
      border: OutlineInputBorder(),
      filled: true,
    ),
  );
}

/// Give the incoming page one spatial spring. The compositor moves its cached
/// layer without rebuilding the page on each frame.
class PageMotion extends StatefulWidget {
  const PageMotion({super.key, required this.page, required this.child});
  final int page;
  final Widget child;
  @override
  State<PageMotion> createState() => _PageMotionState();
}

class _PageMotionState extends State<PageMotion>
    with SingleTickerProviderStateMixin {
  late final _controller = AnimationController.unbounded(vsync: this, value: 0);
  @override
  void didUpdateWidget(PageMotion oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.page == oldWidget.page) return;
    if (MediaQuery.disableAnimationsOf(context)) {
      _controller.value = 0;
      return;
    }
    final start = _controller.value + (widget.page > oldWidget.page ? 24 : -24);
    _controller.animateWith(
      SpringSimulation(
        Springs.expressiveDefaultSpatial,
        start,
        0,
        _controller.velocity,
      ),
    );
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _controller,
    child: RepaintBoundary(child: widget.child),
    builder: (_, child) =>
        Transform.translate(offset: Offset(0, _controller.value), child: child),
  );
  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }
}
