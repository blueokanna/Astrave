import 'package:astrave_desktop/cloudflare.dart';
import 'package:astrave_desktop/overview.dart';
import 'package:astrave_desktop/session.dart';
import 'package:astrave_desktop/src/rust/api.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

void main() {
  testWidgets('connector setup masks token and creates a native profile', (
    tester,
  ) async {
    String? result;
    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => Scaffold(
            body: TextButton(
              onPressed: () async {
                result = await cloudflareProfile(context, (en, _) => en);
              },
              child: const Text('Setup'),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('Setup'));
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<FilledButton>(
            find.widgetWithText(FilledButton, 'Use this configuration'),
          )
          .onPressed,
      isNull,
    );
    final fields = find.byType(TextField);
    expect(tester.widget<TextField>(fields.first).obscureText, isTrue);
    await tester.enterText(fields.first, ' eyJtest-token ');
    await tester.enterText(
      fields.at(1),
      '550e8400-e29b-41d4-a716-446655440000',
    );
    await tester.pump();
    await tester.tap(find.text('Use this configuration'));
    await tester.pumpAndSettle();
    expect(result, contains('[cloudflare]\n'));
    expect(result, contains('token = "eyJtest-token"'));
    expect(result, contains('connections = 4'));
    expect(result, isNot(contains('server =')));
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'overview displays dashboard routes without local tunnel editing',
    (tester) async {
      final session =
          Session(
              const Profile(
                path: '',
                source: '',
                server: 'Cloudflare',
                transport: 'cloudflare/auto',
                tls: true,
                multiplex: true,
                tunnels: [],
              ),
            )
            ..status = 'Running'
            ..remotes = {'web.example.com': 'https://web.example.com'};
      addTearDown(session.dispose);
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Overview(
              session: session,
              text: (en, _) => en,
              connect: () {},
              edit: () {},
              modify: (_, {remove = false}) async {},
            ),
          ),
        ),
      );
      expect(find.textContaining('Cloudflare routes'), findsOneWidget);
      expect(find.textContaining('https://web.example.com'), findsOneWidget);
      expect(find.byTooltip('Add tunnel'), findsNothing);
      expect(find.byTooltip('Edit tunnel'), findsNothing);
      expect(tester.takeException(), isNull);
    },
  );
}
