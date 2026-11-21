import 'dart:ui' as ui;

import 'package:astrave_desktop/navigation_capsule.dart';
import 'package:astrave_desktop/settings.dart';
import 'package:flutter/rendering.dart' show RenderRepaintBoundary;
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

class _Preferences extends Fake implements SharedPreferencesAsync {
  final values = <String, Object>{'chinese': true, 'dark': false};
  @override
  Future<bool?> getBool(String key) async => values[key] as bool?;
  @override
  Future<List<String>?> getStringList(String key) async =>
      values[key] as List<String>?;
  @override
  Future<void> setStringList(String key, List<String> value) async {
    values[key] = value;
  }
}

void main() {
  testWidgets(
    'glass samples scrolling content beneath the large-text capsule',
    (tester) async {
      tester.view.physicalSize = const Size(360, 640);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final boundary = GlobalKey();
      final scroll = ScrollController();
      addTearDown(scroll.dispose);
      Widget view(NavGlass glass) => MaterialApp(
        home: Builder(
          builder: (context) => MediaQuery(
            data: MediaQuery.of(context)
                .copyWith(textScaler: TextScaler.linear(2)),
            child: RepaintBoundary(
              key: boundary,
              child: Scaffold(
                extendBody: true,
                body: ListView(
                  controller: scroll,
                  padding: EdgeInsets.zero,
                  children: [
                    for (var i = 0; i < 4; i++)
                      ColoredBox(
                        color: i.isEven ? Colors.black : Colors.white,
                        child: const SizedBox(height: 640),
                      ),
                  ],
                ),
                bottomNavigationBar: NavigationCapsule(
                  glass: glass,
                  selected: 0,
                  select: (_) {},
                  destinations: const [
                    NavigationDestination(
                      icon: Icon(Icons.home),
                      label: 'Overview',
                    ),
                    NavigationDestination(
                      icon: Icon(Icons.settings),
                      label: 'Settings',
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      );
      Future<int> sample() async {
        final obj =
            boundary.currentContext!.findRenderObject()!
                as RenderRepaintBoundary;
        final image = await obj.toImage();
        try {
          final bytes = (await image.toByteData(
            format: ui.ImageByteFormat.rawRgba,
          ))!;
          return bytes.getUint8(4 * (image.width * 590 + 180));
        } finally {
          image.dispose();
        }
      }

      for (final style in NavGlass.values) {
        await tester.pumpWidget(view(style));
        scroll.jumpTo(0);
        await tester.pumpAndSettle();
        final dark = await tester.runAsync(sample);
        scroll.jumpTo(640);
        await tester.pumpAndSettle();
        final light = await tester.runAsync(sample);
        expect(
          light! - dark!,
          style == NavGlass.solid ? equals(0) : greaterThan(5),
          reason: style.name,
        );
        expect(tester.takeException(), isNull);
      }
    },
  );
  test(
    'upgrades legacy preferences and restores explicit system settings',
    () async {
      final store = _Preferences();
      final old = await Appearance.load(store);
      expect(old.language, AppLanguage.zh);
      expect(old.theme, ThemeMode.light);
      await old
          .copyWith(
            language: AppLanguage.system,
            theme: ThemeMode.system,
            palette: Palette.jade,
            glass: NavGlass.liquid,
          )
          .save(store);
      final next = await Appearance.load(store);
      expect(next.language, AppLanguage.system);
      expect(next.theme, ThemeMode.system);
      expect(next.palette, Palette.jade);
      expect(next.glass, NavGlass.liquid);
    },
  );

  testWidgets(
    'capsule selection updates on a narrow screen and respects high contrast',
    (tester) async {
      var appearance = const Appearance();
      var page = 3;
      var opened = false;
      tester.view.physicalSize = const Size(360, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      Widget view(bool contrast) => MaterialApp(
        home: StatefulBuilder(
          builder: (context, update) => MediaQuery(
            data: MediaQuery.of(context).copyWith(highContrast: contrast),
            child: Scaffold(
              extendBody: true,
              body: SingleChildScrollView(
                child: SettingsPanel(
                  value: appearance,
                  text: (en, zh) => en,
                  onChanged: (v) => update(() => appearance = v),
                  certificates: () => opened = true,
                ),
              ),
              bottomNavigationBar: NavigationCapsule(
                glass: appearance.glass,
                selected: page,
                select: (v) => update(() => page = v),
                destinations: [
                  for (final title in [
                    'Overview',
                    'Profile',
                    'Activity',
                    'Settings',
                  ])
                    NavigationDestination(
                      icon: const Icon(Icons.circle),
                      label: title,
                    ),
                ],
              ),
            ),
          ),
        ),
      );
      await tester.pumpWidget(view(false));
      await tester.tap(find.text('Navigation capsule'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Aero Glass style'));
      await tester.pumpAndSettle();
      expect(appearance.glass, NavGlass.aero);
      expect(find.byType(BackdropFilter), findsOneWidget);
      await tester.tap(find.text('Certificates'));
      expect(opened, isTrue);
      await tester.tap(find.text('Activity'));
      await tester.pumpAndSettle();
      expect(page, 2);
      await tester.pumpWidget(view(true));
      await tester.pumpAndSettle();
      expect(find.byType(BackdropFilter), findsNothing);
    },
  );
}
