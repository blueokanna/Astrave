import 'dart:ui' as ui;

import 'package:material_ui/material_ui.dart';

import 'settings.dart';

class NavigationCapsule extends StatelessWidget {
  const NavigationCapsule({
    super.key,
    required this.glass,
    required this.selected,
    required this.select,
    required this.destinations,
  });
  final NavGlass glass;
  final int selected;
  final ValueChanged<int> select;
  final List<Widget> destinations;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final style = MediaQuery.highContrastOf(context) ? NavGlass.solid : glass;
    final surface = colors.surfaceContainerHigh;
    final gradient = switch (style) {
      NavGlass.aero => LinearGradient(
        begin: Alignment.topLeft,
        end: Alignment.bottomRight,
        stops: const [0, .48, .51, 1],
        colors: [
          colors.secondaryContainer.withValues(alpha: .88),
          surface.withValues(alpha: .71),
          colors.primaryContainer.withValues(alpha: .67),
          surface.withValues(alpha: .85),
        ],
      ),
      NavGlass.liquid => LinearGradient(
        begin: Alignment.topLeft,
        end: Alignment.bottomRight,
        colors: [
          colors.surfaceBright.withValues(alpha: .85),
          surface.withValues(alpha: .56),
          colors.surfaceBright.withValues(alpha: .72),
        ],
      ),
      _ => null,
    };
    final largeText = MediaQuery.textScalerOf(context).scale(12) > 16;
    final nav = DecoratedBox(
      decoration: ShapeDecoration(
        shape: StadiumBorder(
          side: BorderSide(color: colors.outlineVariant.withValues(alpha: .85)),
        ),
        color: gradient == null
            ? surface.withValues(alpha: style == NavGlass.solid ? 1 : .8)
            : null,
        gradient: gradient,
      ),
      child: MediaQuery.removePadding(
        context: context,
        removeBottom: true,
        child: NavigationBar(
          height: 68,
          backgroundColor: Colors.transparent,
          surfaceTintColor: Colors.transparent,
          elevation: 0,
          labelBehavior: largeText
              ? NavigationDestinationLabelBehavior.alwaysHide
              : NavigationDestinationLabelBehavior.alwaysShow,
          selectedIndex: selected,
          onDestinationSelected: select,
          destinations: destinations,
        ),
      ),
    );
    final sigma = style == NavGlass.aero
        ? 16.0
        : style == NavGlass.liquid
        ? 5.0
        : 10.0;
    final blur = ui.ImageFilter.blur(sigmaX: sigma, sigmaY: sigma);
    final child = switch (style) {
      NavGlass.solid => nav,
      NavGlass.liquid when ui.ImageFilter.isShaderFilterSupported =>
        _RefractedGlass(blur: blur, child: nav),
      _ => BackdropFilter(filter: blur, child: nav),
    };
    return SafeArea(
      top: false,
      minimum: const EdgeInsets.fromLTRB(12, 0, 12, 8),
      child: Padding(
        padding: const EdgeInsets.only(top: 6, bottom: 4),
        child: Align(
          heightFactor: 1,
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 440),
            child: DecoratedBox(
              decoration: ShapeDecoration(
                shape: const StadiumBorder(),
                shadows: [
                  BoxShadow(
                    color: colors.shadow.withValues(alpha: .15),
                    blurRadius: 10,
                    offset: const Offset(0, 3),
                  ),
                ],
              ),
              child: ClipPath(
                clipper: const ShapeBorderClipper(shape: StadiumBorder()),
                child: child,
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _RefractedGlass extends StatefulWidget {
  const _RefractedGlass({required this.blur, required this.child});
  final ui.ImageFilter blur;
  final Widget child;
  @override
  State<_RefractedGlass> createState() => _RefractedGlassState();
}

class _RefractedGlassState extends State<_RefractedGlass> {
  static final program = ui.FragmentProgram.fromAsset('shaders/capsule.frag');
  ui.FragmentShader? shader;
  @override
  void initState() {
    super.initState();
    program.then(
      (p) {
        if (mounted) setState(() => shader = p.fragmentShader());
      },
      onError: (Object error, StackTrace stack) {
        FlutterError.reportError(
          FlutterErrorDetails(exception: error, stack: stack),
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) => BackdropFilter(
    filter: shader == null
        ? widget.blur
        : ui.ImageFilter.compose(
            outer: ui.ImageFilter.shader(shader!),
            inner: widget.blur,
          ),
    child: widget.child,
  );
  @override
  void dispose() {
    shader?.dispose();
    super.dispose();
  }
}
