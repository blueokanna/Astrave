import 'package:astrave_desktop/design.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

void main() {
  testWidgets('page motion follows elapsed vsync time across refresh modes', (
    tester,
  ) async {
    final display = tester.view.display;
    addTearDown(display.resetRefreshRate);
    Widget view(int page) => MaterialApp(
      home: PageMotion(page: page, child: Text('page $page')),
    );
    Future<double> positionAfter(List<int> modes) async {
      await tester.pumpWidget(const SizedBox());
      await tester.pumpWidget(view(0));
      await tester.pumpWidget(view(1));
      await tester.pump();
      var elapsed = 0;
      for (final hz in modes) {
        display.refreshRate = hz.toDouble();
        final step = (1000000 / hz).round();
        final end = elapsed + 100000 ~/ modes.length;
        while (elapsed < end) {
          final delta = (end - elapsed).clamp(1, step);
          final before = tester.getTopLeft(find.text('page 1')).dx;
          await tester.pump(Duration(microseconds: delta));
          elapsed += delta;
          expect(tester.getTopLeft(find.text('page 1')).dx, lessThan(before));
        }
      }
      return tester.getTopLeft(find.text('page 1')).dx;
    }

    final at60 = await positionAfter([60]);
    for (final modes in [
      [90],
      [120],
      [60, 120],
    ]) {
      expect(await positionAfter(modes), closeTo(at60, 0.01));
    }
    await tester.pumpAndSettle();
  });

  testWidgets(
    'rapid tab reversal preserves state and releases outgoing pages',
    (tester) async {
      Widget view(int page, {bool reduced = false}) => MaterialApp(
        home: MediaQuery(
          data: MediaQueryData(disableAnimations: reduced),
          child: Material(
            child: PageMotion(
              page: page,
              child: page == 0 ? const TextField() : const Text('Profile'),
            ),
          ),
        ),
      );
      await tester.pumpWidget(view(0));
      await tester.enterText(find.byType(TextField), 'draft');
      await tester.pumpWidget(view(1));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 32));
      final origin = tester.getTopLeft(find.byType(TextField));
      final destination = tester.getTopLeft(find.text('Profile'));
      await tester.pumpWidget(view(0));
      expect(tester.getTopLeft(find.byType(TextField)), origin);
      expect(tester.getTopLeft(find.text('Profile')), destination);
      await tester.pumpAndSettle();
      expect(find.text('draft'), findsOneWidget);
      expect(find.text('Profile'), findsNothing);
      await tester.pumpWidget(view(1, reduced: true));
      expect(find.byType(TextField), findsNothing);
      expect(find.text('Profile'), findsOneWidget);
    },
  );
}
