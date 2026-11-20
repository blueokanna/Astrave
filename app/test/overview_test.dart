import 'package:astrave_desktop/design.dart';
import 'package:astrave_desktop/overview.dart';
import 'package:astrave_desktop/session.dart';
import 'package:astrave_desktop/src/rust/api.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

void main() {
  testWidgets(
    'overview renders the configured transport settings at narrow widths',
    (tester) async {
      tester.view.physicalSize = const Size(360, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final session = Session(
        const Profile(
          path: '',
          source: '',
          server: 'localhost:9527',
          transport: 'quic',
          tls: false,
          multiplex: false,
          tunnels: [],
        ),
      );
      addTearDown(session.dispose);
      await tester.pumpWidget(
        MaterialApp(
          theme: desktopTheme(Brightness.light, null),
          home: Scaffold(
            body: Overview(
              session: session,
              text: (en, _) => en,
              connect: () {},
              edit: () {},
            ),
          ),
        ),
      );
      expect(find.text('TLS'), findsOneWidget);
      expect(find.text('QUIC'), findsNWidgets(2));
      expect(find.text('Disconnected'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'large profiles build visible routes and hide stale endpoints after a save',
    (tester) async {
      final tunnels = List.generate(
        1000,
        (i) => Tunnel(
          name: 'route-$i',
          protocol: 'tcp',
          service: 'localhost:8000',
          publicEndpoint: '0',
        ),
      );
      Profile profile(String source) => Profile(
        path: '',
        source: source,
        server: 'localhost:9527',
        transport: 'tcp',
        tls: true,
        multiplex: true,
        tunnels: tunnels,
      );
      final session = Session(profile('first'))
        ..status = 'Running'
        ..remotes = {'route-0': '127.0.0.1:2345'};
      addTearDown(session.dispose);
      Future<void> render() => tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Overview(
              session: session,
              text: (en, _) => en,
              connect: () {},
              edit: () {},
            ),
          ),
        ),
      );
      await render();
      expect(find.textContaining('127.0.0.1:2345'), findsOneWidget);
      expect(find.text('route-999', skipOffstage: false), findsNothing);
      expect(
        find.textContaining('route-', skipOffstage: false).evaluate().length,
        lessThan(40),
      );
      session.saved(profile('second'));
      await render();
      expect(find.textContaining('127.0.0.1:2345'), findsNothing);
      expect(find.textContaining('pending reload'), findsOneWidget);
      await tester.scrollUntilVisible(
        find.text('route-20'),
        350,
        scrollable: find
            .byWidgetPredicate(
              (w) => w is Scrollable && w.axisDirection == AxisDirection.down,
            )
            .first,
      );
      expect(find.text('route-20'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );
}
