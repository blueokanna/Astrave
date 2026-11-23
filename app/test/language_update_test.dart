import 'dart:convert';
import 'dart:ffi' show Abi;
import 'dart:io';

import 'package:astrave_desktop/design.dart';
import 'package:astrave_desktop/language.dart';
import 'package:astrave_desktop/settings.dart';
import 'package:astrave_desktop/updates.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:package_info_plus/package_info_plus.dart';

class _Network extends HttpOverrides {}

class _LoopbackClient extends Fake implements HttpClient {
  _LoopbackClient(this.port);
  int port;
  final client = HttpClient();
  @override
  Future<HttpClientRequest> getUrl(Uri uri) =>
      client.getUrl(Uri.http('127.0.0.1:$port', uri.path));
  @override
  void close({bool force = false}) => client.close(force: force);
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late Messages messages;
  setUpAll(() async {
    messages = await Messages.load();
  });
  test(
    'catalog covers UI messages, placeholders and preferred locales',
    () async {
      expect(
        messages.catalogs.keys.toSet(),
        AppLanguage.values.skip(3).toSet(),
      );
      final literals = RegExp(
        r'''\b(?:text|t)\(\s*'([^']+)'\s*,\s*'[^']+',?\s*\)''',
      );
      final keys = {
        for (final file in Directory(
          'lib',
        ).listSync().whereType<File>().where((f) => f.path.endsWith('.dart')))
          ...literals.allMatches(file.readAsStringSync()).map((m) => m[1]!),
      };
      expect(keys.length, greaterThan(105));
      for (final catalog in messages.catalogs.values) {
        expect(keys.difference(catalog.keys.toSet()), isEmpty);
        for (final key in keys.where((k) => k.contains('{name}'))) {
          expect('{name}'.allMatches(catalog[key]!), hasLength(1));
        }
      }
      expect(
        AppLanguage.system.resolve([const Locale('zh', 'HK')]),
        AppLanguage.zhHant,
      );
      expect(
        AppLanguage.system.resolve([const Locale('xx'), const Locale('ar')]),
        AppLanguage.ar,
      );
      expect(
        AppLanguage.system.resolve([
          const Locale.fromSubtags(
            languageCode: 'zh',
            scriptCode: 'Hans',
            countryCode: 'TW',
          ),
        ]),
        AppLanguage.zh,
      );
    },
  );

  testWidgets(
    'Arabic uses RTL Material controls and the Monet accent survives black surfaces',
    (tester) async {
      final system = ColorScheme.fromSeed(
        seedColor: Colors.teal,
        brightness: Brightness.dark,
      );
      final theme = desktopTheme(
        Brightness.dark,
        system,
        seed: Colors.red,
        blackSurface: true,
        preferDynamic: true,
      );
      expect(theme.scaffoldBackgroundColor, Colors.black);
      expect(theme.colorScheme.primary, system.primary);
      tester.view.physicalSize = const Size(340, 740);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      for (final language in [AppLanguage.ar, AppLanguage.de]) {
        String text(String en, String zh) => messages.text(language, en, zh);
        await tester.pumpWidget(
          MaterialApp(
            key: ValueKey(language),
            theme: theme,
            locale: language.locale,
            supportedLocales: [language.locale!],
            localizationsDelegates: GlobalMaterialLocalizations.delegates,
            builder: (context, child) => MediaQuery(
              data: MediaQuery.of(context)
                  .copyWith(textScaler: TextScaler.linear(1.8)),
              child: child!,
            ),
            home: Scaffold(
              body: SingleChildScrollView(
                child: SettingsPanel(
                  value: Appearance(
                    language: language,
                    theme: AppTheme.amoledMonet,
                  ),
                  text: text,
                  onChanged: (_) {},
                  certificates: () {},
                  about: () {},
                  updates: const SizedBox(),
                ),
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        final palette = find.text(text('Color palette', '色彩方案'));
        expect(
          Directionality.of(tester.element(palette)),
          language == AppLanguage.ar ? TextDirection.rtl : TextDirection.ltr,
        );
        await tester.ensureVisible(palette);
        await tester.tap(palette);
        await tester.pumpAndSettle();
        expect(find.byType(ChoiceChip), findsNWidgets(Palette.values.length));
        expect(tester.takeException(), isNull);
      }
    },
  );

  test(
    'release asset selection and streaming verification fail closed',
    () async {
      final bytes = [1, 2, 3, 4, 5];
      final manifest = {
        'tag_name': 'v1.2.10',
        'draft': false,
        'prerelease': false,
        'assets': [
          {
            'name': 'astrave_1.2.10_android_arm64-v8a.apk',
            'state': 'uploaded',
            'digest': 'sha256:${sha256.convert(bytes)}',
            'size': bytes.length,
            'browser_download_url': 'https://example.org/wrong.apk',
          },
        ],
      };
      final release = PublishedRelease.fromJson(manifest, Abi.androidArm64);
      expect(
        release.url.toString(),
        'https://github.com/blueokanna/Astrave/releases/download/v1.2.10/astrave_1.2.10_android_arm64-v8a.apk',
      );
      expect(PublishedRelease.fromJson(manifest, Abi.windowsX64).sha, isNull);
      expect(
        () => PublishedRelease.fromJson({
          ...manifest,
          'draft': true,
        }, Abi.androidArm64),
        throwsFormatException,
      );
      final dir = await Directory.systemTemp.createTemp('astrave-verify-');
      addTearDown(() => dir.delete(recursive: true));
      final file = File('${dir.path}/update.apk');
      await release.verifyTo(
        Stream.fromIterable([
          [1, 2],
          [3, 4, 5],
        ]),
        file,
      );
      expect(await file.readAsBytes(), bytes);
      for (final bad in [
        [1, 2],
        [5, 4, 3, 2, 1],
        [1, 2, 3, 4, 5, 6],
      ]) {
        await expectLater(
          release.verifyTo(Stream.value(bad), file),
          throwsFormatException,
        );
      }
    },
  );

  test('conditional release checks preserve the previous version across offline retries', () async {
    await HttpOverrides.runWithHttpOverrides(() async {
      PackageInfo.setMockInitialValues(
        appName: 'Astrave',
        packageName: 'astrave',
        version: '1.0.1',
        buildNumber: '2',
        buildSignature: '',
      );
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      final tags = <String?>[];
      server.listen((request) async {
        tags.add(request.headers.value(HttpHeaders.ifNoneMatchHeader));
        request.response.statusCode = tags.length == 1
            ? HttpStatus.ok
            : HttpStatus.notModified;
        request.response.headers.set(HttpHeaders.etagHeader, '"fixture"');
        if (tags.length == 1) {
          request.response.write(
            jsonEncode({
              'tag_name': 'v1.0.12',
              'draft': false,
              'prerelease': false,
              'assets': [],
            }),
          );
        }
        await request.response.close();
      });
      final transport = _LoopbackClient(server.port);
      final updates = ReleaseUpdates(client: transport);
      addTearDown(updates.dispose);
      try {
        await updates.check();
        final latest = updates.latest;
        expect(latest?.version.toString(), '1.0.12');
        await updates.check();
        expect(identical(updates.latest, latest), isTrue);
        expect(updates.error, isNull);
        expect(tags, [null, '"fixture"']);
      } finally {
        await server.close(force: true);
      }
      await updates.check();
      expect(updates.error, isNotNull);
      expect(updates.latest?.version.toString(), '1.0.12');
      expect(updates.checking, isFalse);
      // Keep the logical release URL while routing recovery to a fresh socket.
      // Windows can still reject a TCP connection to the recently closed port.
      final restored = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      transport.port = restored.port;
      try {
        String? previousTag;
        restored.listen((request) async {
          previousTag = request.headers.value(HttpHeaders.ifNoneMatchHeader);
          request.response.headers.set(HttpHeaders.etagHeader, '"new-release"');
          request.response.write(jsonEncode({
            'tag_name': 'v1.0.13',
            'draft': false,
            'prerelease': false,
            'assets': [],
          }));
          await request.response.close();
        });
        await updates.check();
        expect(previousTag, '"fixture"');
        expect(updates.latest?.version.toString(), '1.0.13');
        expect(updates.error, isNull);
      } finally {
        await restored.close(force: true);
      }
    }, _Network());
  });
}
