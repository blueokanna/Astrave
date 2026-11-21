import 'package:astrave_desktop/activity.dart';
import 'package:astrave_desktop/session.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

void main() {
  testWidgets('the filtered view follows new batches, eviction and clear', (
    tester,
  ) async {
    final logs = SessionLog()..append(['WARN oldest']);
    final filter = TextEditingController(text: 'WaRn');
    addTearDown(logs.dispose);
    addTearDown(filter.dispose);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: ActivityPanel(logs: logs, filter: filter, text: (en, _) => en),
        ),
      ),
    );
    expect(find.text('oldest'), findsOneWidget);
    logs.append([...List.filled(SessionLog.limit - 1, 'INFO'), 'WARN newest']);
    await tester.pump();
    expect(find.text('oldest'), findsNothing);
    expect(find.text('newest'), findsOneWidget);
    expect(find.text('WARN'), findsOneWidget);
    expect(find.text('INFO'), findsNothing);
    await tester.tap(find.byTooltip('Clear logs'));
    await tester.pump();
    expect(find.text('No matching events'), findsOneWidget);
    expect(filter.text, 'WaRn');
  });
}
