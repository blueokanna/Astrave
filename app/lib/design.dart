import 'package:flutter/physics.dart';
import 'package:material_ui/material_ui.dart';

typedef LocalText = String Function(String english, String chinese);

abstract final class Space {
  static const small = 8.0;
  static const medium = 16.0;
  static const large = 24.0;
  static const section = 32.0;
}

ThemeData desktopTheme(
  Brightness brightness,
  ColorScheme? dynamicScheme, {
  Color? seed,
  bool blackSurface = false,
  bool preferDynamic = false,
  bool simplifiedChinese = true,
}) {
  final base =
      (preferDynamic || seed == null ? dynamicScheme : null) ??
      ColorScheme.fromSeed(
        seedColor: seed ?? const Color(0xff6651a8),
        brightness: brightness,
        dynamicSchemeVariant: DynamicSchemeVariant.expressive,
      );
  final scheme = blackSurface
      ? base.copyWith(
          surface: Colors.black,
          surfaceDim: Colors.black,
          surfaceContainerLowest: Colors.black,
          surfaceContainerLow: Color.alphaBlend(
            base.onSurface.withValues(alpha: .05),
            Colors.black,
          ),
          surfaceContainer: Color.alphaBlend(
            base.onSurface.withValues(alpha: .09),
            Colors.black,
          ),
        )
      : base;
  return ThemeData(
    useMaterial3: true,
    fontFamily: 'GoogleSans',
    fontFamilyFallback: simplifiedChinese ? const ['HarmonyOSSans'] : null,
    colorScheme: scheme,
    pageTransitionsTheme: const PageTransitionsTheme(
      builders: {
        TargetPlatform.android: PredictiveBackPageTransitionsBuilder(),
        TargetPlatform.linux: FadeForwardsPageTransitionsBuilder(),
        TargetPlatform.windows: FadeForwardsPageTransitionsBuilder(),
        TargetPlatform.macOS: FadeForwardsPageTransitionsBuilder(),
      },
    ),
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
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        minimumSize: const Size(64, 48),
        padding: const EdgeInsets.symmetric(horizontal: 20),
        shape: const StadiumBorder(),
      ),
    ),
    iconButtonTheme: const IconButtonThemeData(
      variant: StyleVariant.material3Expressive,
    ),
    inputDecorationTheme: InputDecorationThemeData(
      border: OutlineInputBorder(borderRadius: BorderRadius.circular(16)),
      filled: true,
    ),
  );
}

class AppIcon extends StatelessWidget {
  const AppIcon({super.key});

  @override
  Widget build(BuildContext context) => Image.asset(
    'assets/astrave.png',
    width: 40,
    cacheWidth: (40 * MediaQuery.devicePixelRatioOf(context)).ceil(),
  );
}

/// One vsync drives both page layers, including a reversal during navigation.
class PageMotion extends StatefulWidget {
  const PageMotion({super.key, required this.page, required this.child});
  final int page;
  final Widget child;
  @override
  State<PageMotion> createState() => _PageMotionState();
}

class _PageMotionState extends State<PageMotion>
    with SingleTickerProviderStateMixin {
  late final _controller = AnimationController.unbounded(vsync: this, value: 1)
    ..addStatusListener((status) {
      if (status == AnimationStatus.completed && _previous != null) {
        setState(() => _previous = null);
      }
    });
  PageMotion? _previous;
  double _inOffset = 0, _outOffset = 0, _outEnd = 0;
  double _inOpacity = 1, _outOpacity = 0;

  double _mix(double start, double end) =>
      start + (end - start) * _controller.value;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (MediaQuery.disableAnimationsOf(context)) {
      _previous = null;
      _controller.value = 1;
    }
  }

  @override
  void didUpdateWidget(PageMotion oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.page == oldWidget.page ||
        MediaQuery.disableAnimationsOf(context)) {
      return;
    }
    final reversing = widget.page == _previous?.page;
    final forward = widget.page > oldWidget.page;
    final step = forward == (Directionality.of(context) == TextDirection.ltr)
        ? 28.0
        : -28.0;
    final incomingOffset = reversing ? _mix(_outOffset, _outEnd) : step;
    final incomingOpacity = reversing
        ? _PagePainter.fadeOut(_outOpacity, _controller.value)
        : 0.0;
    _outOffset = _mix(_inOffset, 0);
    _outOpacity = _PagePainter.fadeIn(_inOpacity, _controller.value);
    _inOffset = incomingOffset;
    _inOpacity = incomingOpacity;
    _outEnd = -step;
    _previous = oldWidget;
    _controller.animateWith(
      SpringSimulation(
        Springs.expressiveDefaultSpatial,
        0,
        1,
        0,
        snapToEnd: true,
      ),
    );
  }

  Widget _layer(PageMotion page, {required bool entering}) => IgnorePointer(
    key: ValueKey(page.page),
    ignoring: !entering,
    child: ExcludeSemantics(
      excluding: !entering,
      child: ExcludeFocus(
        excluding: !entering,
        child: TickerMode(enabled: entering, child: page.child),
      ),
    ),
  );

  @override
  Widget build(BuildContext context) => Flow(
    delegate: _PagePainter(
      _controller,
      _inOffset,
      _outOffset,
      _outEnd,
      _inOpacity,
      _outOpacity,
    ),
    children: [
      if (_previous case final previous?) _layer(previous, entering: false),
      _layer(widget, entering: true),
    ],
  );
  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }
}

class _PagePainter extends FlowDelegate {
  _PagePainter(
    this.progress,
    this.inOffset,
    this.outOffset,
    this.outEnd,
    this.inOpacity,
    this.outOpacity,
  ) : super(repaint: progress);

  final Animation<double> progress;
  final double inOffset, outOffset, outEnd, inOpacity, outOpacity;

  // Material fade-through releases the outgoing opacity layer before fading in the next page.
  static const _fadeThrough = .35;
  static double fadeOut(double start, double t) =>
      start * (1 - (t / _fadeThrough).clamp(0.0, 1.0));
  static double fadeIn(double start, double t) =>
      start +
      (1 - start) * ((t - _fadeThrough) / (1 - _fadeThrough)).clamp(0.0, 1.0);

  @override
  void paintChildren(FlowPaintingContext context) {
    if (context.childCount == 1) {
      context.paintChild(0);
      return;
    }
    final t = progress.value;
    void paint(int index, double x, double opacity) => context.paintChild(
      index,
      transform: Matrix4.translationValues(x, 0, 0),
      opacity: opacity.clamp(0.0, 1.0),
    );
    paint(0, outOffset + (outEnd - outOffset) * t, fadeOut(outOpacity, t));
    paint(1, inOffset * (1 - t), fadeIn(inOpacity, t));
  }

  @override
  bool shouldRepaint(_PagePainter oldDelegate) =>
      inOffset != oldDelegate.inOffset ||
      outOffset != oldDelegate.outOffset ||
      outEnd != oldDelegate.outEnd ||
      inOpacity != oldDelegate.inOpacity ||
      outOpacity != oldDelegate.outOpacity;
}

class ActionsMenu extends StatelessWidget {
  const ActionsMenu({super.key, required this.tooltip, required this.children});
  final String tooltip;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) => MenuAnchor(
    menuChildren: children,
    builder: (_, controller, _) => IconButton(
      tooltip: tooltip,
      icon: const Icon(Icons.more_vert),
      onPressed: () =>
          controller.isOpen ? controller.close() : controller.open(),
    ),
  );
}
