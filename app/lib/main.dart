import 'dart:io';

import 'package:dynamic_color/dynamic_color.dart';
import 'package:file_selector/file_selector.dart' as files;
import 'package:flutter/foundation.dart'
    show LicenseEntryWithLineBreaks, LicenseRegistry;
import 'package:flutter/services.dart' show rootBundle;
import 'package:material_ui/material_ui.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'activity.dart';
import 'android_profile.dart';
import 'android_tls.dart';
import 'certificates.dart';
import 'design.dart';
import 'desktop_tray.dart';
import 'overview.dart';
import 'session.dart';
import 'src/rust/api.dart' as rust;
import 'src/rust/frb_generated.dart';

void main(List<String> arguments) {
  WidgetsFlutterBinding.ensureInitialized();
  LicenseRegistry.addLicense(() async* {
    for (final (family, file) in const [
      ('Google Sans', 'GoogleSans-OFL.txt'),
      ('HarmonyOS Sans', 'HarmonyOS-Sans-License.txt'),
    ]) {
      final text = await rootBundle.loadString('assets/fonts/$file');
      yield LicenseEntryWithLineBreaks([family], text);
    }
  });
  runApp(AstraveApp(profilePath: arguments.firstOrNull));
}

class AstraveApp extends StatefulWidget {
  const AstraveApp({super.key, this.profilePath});
  final String? profilePath;
  @override
  State<AstraveApp> createState() => _AstraveAppState();
}

class _AstraveAppState extends State<AstraveApp> {
  late final Future<String> _defaultPath = _initialize();
  late bool chinese =
      WidgetsBinding.instance.platformDispatcher.locale.languageCode == 'zh';
  ThemeMode mode = ThemeMode.system;
  final prefs = SharedPreferencesAsync();
  DesktopTray? tray;

  Future<String> _initialize() async {
    await RustLib.init();
    if (Platform.isAndroid) initializeAndroidTls();
    final dir = await getApplicationSupportDirectory();
    final path = await rust.initialize(directory: dir.path);
    chinese = await prefs.getBool('chinese') ?? chinese;
    final dark = await prefs.getBool('dark');
    if (dark != null) mode = dark ? ThemeMode.dark : ThemeMode.light;
    if (mounted) {
      if (Platform.isWindows || Platform.isLinux || Platform.isMacOS) {
        tray = DesktopTray()..install(chinese);
      }
      setState(() {});
    }
    return path;
  }

  @override
  void dispose() {
    tray?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => DynamicColorBuilder(
    builder: (light, dark) => MaterialApp(
      title: 'Astrave',
      debugShowCheckedModeBanner: false,
      theme: desktopTheme(Brightness.light, light),
      darkTheme: desktopTheme(Brightness.dark, dark),
      themeMode: mode,
      themeAnimationStyle: MediaQuery.disableAnimationsOf(context)
          ? AnimationStyle.noAnimation
          : const AnimationStyle(
              duration: Durations.medium2,
              curve: Easing.standard,
            ),
      home: FutureBuilder<String>(
        future: _defaultPath,
        builder: (context, state) {
          if (state.hasError) {
            return Scaffold(
              body: Center(child: SelectableText(state.error.toString())),
            );
          }
          if (!state.hasData) {
            return const Scaffold(
              body: Center(child: CircularProgressIndicator()),
            );
          }
          return Workspace(
            initialPath: widget.profilePath ?? state.requireData,
            text: (en, zh) => chinese ? zh : en,
            hide: tray?.visible == true ? tray!.hide : null,
            language: () async {
              setState(() => chinese = !chinese);
              tray?.localize(chinese);
              await prefs.setBool('chinese', chinese);
            },
            theme: () async {
              final dark = Theme.of(context).brightness == Brightness.light;
              setState(() => mode = dark ? ThemeMode.dark : ThemeMode.light);
              await prefs.setBool('dark', dark);
            },
          );
        },
      ),
    ),
  );
}

class Workspace extends StatefulWidget {
  const Workspace({
    super.key,
    required this.initialPath,
    required this.text,
    required this.language,
    required this.theme,
    this.hide,
  });
  final String initialPath;
  final LocalText text;
  final VoidCallback language, theme;
  final VoidCallback? hide;
  @override
  State<Workspace> createState() => _WorkspaceState();
}

class _WorkspaceState extends State<Workspace> with WidgetsBindingObserver {
  Session? session;
  String? failure;
  int page = 0;
  final editor = TextEditingController();
  final logFilter = TextEditingController();
  LocalText get t => widget.text;
  List<String> get titles => [
    t('Overview', '概览'),
    t('Profile', '配置'),
    t('Activity', '活动日志'),
    t('Certificates', '证书'),
  ];
  static const icons = [
    Icons.space_dashboard_outlined,
    Icons.tune_rounded,
    Icons.subject_rounded,
    Icons.verified_user_outlined,
  ];

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _open(widget.initialPath);
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (Platform.isAndroid) {
      session?.setPollingEnabled(state == AppLifecycleState.resumed);
    }
  }

  Future<void> _open(String path) async {
    try {
      final profile = await rust.openProfile(path: path);
      if (!mounted) return;
      session?.dispose();
      final next = Session(profile);
      if (Platform.isAndroid &&
          WidgetsBinding.instance.lifecycleState == AppLifecycleState.paused) {
        next.setPollingEnabled(false);
      }
      editor.text = profile.source;
      setState(() {
        session = next;
        failure = null;
      });
      await next.refresh();
    } catch (e) {
      if (!mounted) return;
      if (session == null) {
        setState(() => failure = e.toString());
      } else {
        notice(e.toString());
      }
    }
  }

  void notice(String text) {
    if (mounted) {
      ScaffoldMessenger.of(context)
        ..hideCurrentSnackBar()
        ..showSnackBar(SnackBar(content: Text(text)));
    }
  }

  Future<void> act(Future<void> Function() action) async {
    try {
      await session!.change(action);
    } catch (e) {
      notice(e.toString());
    }
  }

  Future<void> openFile() async {
    try {
      final s = session;
      if (s != null &&
          editor.text != s.profile.source &&
          !await confirm(
            context,
            t('Discard unsaved edits?', '放弃尚未保存的更改？'),
            t,
          )) {
        return;
      }
      if (!Platform.isAndroid) {
        final file = await files.openFile(
          acceptedTypeGroups: const [
            files.XTypeGroup(label: 'TOML', extensions: ['toml']),
          ],
        );
        if (file != null && mounted) await _open(file.path);
        return;
      }
      final source = await pickAndroidProfile();
      if (source == null || !mounted) return;
      if (s == null) {
        // Recover an unreadable profile using the same validated, atomic save.
        final path = widget.initialPath;
        if (!await confirm(
          context,
          t('Replace the local profile?', '替换本机配置？'),
          t,
        )) {
          return;
        }
        await rust.saveProfile(
          path: path,
          original: await readProfileText(File(path)),
          source: source,
        );
        await _open(path);
      } else {
        editor.text = source;
        notice(
          t(
            'Review the imported profile, then validate and save.',
            '请检查导入的配置，然后校验并保存。',
          ),
        );
      }
    } catch (e) {
      notice(e.toString());
    }
  }

  Widget profilePanel(Session s) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      SelectableText(
        s.profile.path,
        style: Theme.of(context).textTheme.bodyMedium,
      ),
      const SizedBox(height: Space.medium),
      TextField(
        controller: editor,
        minLines: 10,
        maxLines: 24,
        autocorrect: false,
        enableSuggestions: false,
        style: const TextStyle(fontFamily: 'monospace'),
        decoration: const InputDecoration(
          labelText: 'TOML',
          alignLabelWithHint: true,
        ),
      ),
      const SizedBox(height: Space.medium),
      Wrap(
        spacing: 8,
        runSpacing: 8,
        children: [
          FilledButton.icon(
            icon: const Icon(Icons.save_outlined),
            label: Text(t('Validate & save', '校验并保存')),
            onPressed: s.busy
                ? null
                : () => act(() async {
                    s.saved(
                      await rust.saveProfile(
                        path: s.profile.path,
                        original: s.profile.source,
                        source: editor.text,
                      ),
                    );
                    notice(t('Profile saved', '配置已保存'));
                  }),
          ),
          OutlinedButton.icon(
            icon: const Icon(Icons.folder_open),
            label: Text(
              Platform.isAndroid
                  ? t('Import profile', '导入配置')
                  : t('Open file', '打开文件'),
            ),
            onPressed: s.active || s.busy ? null : openFile,
          ),
          if (s.active)
            OutlinedButton.icon(
              icon: const Icon(Icons.refresh),
              label: Text(t('Reload saved profile', '重载已保存的配置')),
              onPressed: s.busy
                  ? null
                  : () => act(() async {
                      notice(await s.reload());
                    }),
            ),
          TextButton(
            child: Text(t('Reset connection settings', '重置连接设置')),
            onPressed: () async {
              if (!await confirm(
                context,
                t('Reset connection settings in the editor?', '重置编辑器中的连接设置？'),
                t,
              )) {
                return;
              }
              try {
                editor.text = await rust.resetConnectionSettings(
                  source: editor.text,
                );
              } catch (e) {
                notice(e.toString());
              }
            },
          ),
        ],
      ),
    ],
  );

  Future<void> modifyTunnel(int? index, {bool remove = false}) async {
    final s = session!;
    if (editor.text != s.profile.source) {
      notice(t('Save the open profile first.', '请先保存当前配置。'));
      return;
    }
    try {
      String? fragment;
      if (remove) {
        if (!await confirm(context, t('Remove this tunnel?', '删除这条隧道？'), t)) {
          return;
        }
      } else {
        final initial = index == null
            ? '[[tunnels]]\nname = ""\nprotocol = "tcp"\nservice = "127.0.0.1:8000"\nremotePort = 0\n'
            : await rust.tunnelSource(source: s.profile.source, index: index);
        if (!mounted) return;
        fragment = await prompt(
          context,
          t('Tunnel settings', '隧道设置'),
          'TOML',
          initial,
          t,
          multiline: true,
        );
        if (fragment == null) return;
      }
      await act(() async {
        final source = await rust.editTunnel(
          source: s.profile.source,
          index: index,
          fragment: fragment,
        );
        s.saved(
          await rust.saveProfile(
            path: s.profile.path,
            original: s.profile.source,
            source: source,
          ),
        );
        editor.text = s.profile.source;
        if (s.active) await s.reload();
      });
    } catch (e) {
      notice(e.toString());
    }
  }

  Future<void> about() async {
    try {
      final usage = await rust.processUsage();
      if (!mounted) return;
      showAboutDialog(
        context: context,
        applicationName: 'Astrave',
        applicationLegalese: t(
          'Uses Google Sans and HarmonyOS Sans fonts.',
          '使用 Google Sans 和 HarmonyOS Sans 字体。',
        ),
        applicationIcon: const AppIcon(),
        children: [
          Text('CPU  ${usage.cpuPercent.toStringAsFixed(1)}%'),
          Text(
            '${t('Working set', '驻留内存')}  ${usage.memoryMib.toStringAsFixed(1)} MiB',
          ),
          if (session?.connectedSince case final start?)
            Text(
              '${t('Session uptime', '连接时长')}  ${DateTime.now().difference(start).toString().split('.').first}',
            ),
        ],
      );
    } catch (e) {
      notice(e.toString());
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = session;
    final wide = MediaQuery.sizeOf(context).width >= 800;
    final actions = [
      IconButton(
        tooltip: t('Change language', '切换语言'),
        onPressed: widget.language,
        icon: const Icon(Icons.translate),
      ),
      IconButton(
        tooltip: t('Change theme', '切换主题'),
        onPressed: widget.theme,
        icon: const Icon(Icons.contrast),
      ),
      ActionsMenu(
        tooltip: t('More', '更多'),
        children: [
          MenuItemButton(
            onPressed: about,
            child: Text(t('About Astrave', '关于 Astrave')),
          ),
          if (widget.hide != null)
            MenuItemButton(
              onPressed: widget.hide,
              child: Text(t('Hide to tray', '隐藏到托盘')),
            ),
        ],
      ),
    ];
    if (s == null || failure != null) {
      return Scaffold(
        appBar: AppBar(title: const Text('Astrave')),
        body: Center(
          child: failure == null
              ? const CircularProgressIndicator()
              : Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    SelectableText(failure!),
                    TextButton(
                      onPressed: openFile,
                      child: Text(t('Open profile', '打开配置')),
                    ),
                  ],
                ),
        ),
      );
    }
    final heading = Row(
      children: [
        Expanded(
          child: Text(
            titles[page],
            style: Theme.of(context).textTheme.headlineLarge
                ?.copyWith(fontWeight: FontWeight.w600),
          ),
        ),
        if (!wide) ...actions,
      ],
    );
    final panels = switch (page) {
      0 => ListenableBuilder(
        listenable: s,
        child: heading,
        builder: (context, heading) => Overview(
          session: s,
          heading: heading,
          text: t,
          edit: () => setState(() => page = 1),
          modify: modifyTunnel,
          connect: () => act(() async {
            if (s.active) {
              await rust.stopClient();
            } else {
              await rust.startClient(path: s.profile.path);
            }
          }),
        ),
      ),
      1 => ListenableBuilder(listenable: s, builder: (_, _) => profilePanel(s)),
      2 => ActivityPanel(logs: s.logs, filter: logFilter, text: t),
      _ => Certificates(text: t, profilePath: s.profile.path),
    };
    return Scaffold(
      body: SafeArea(
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (wide)
              NavigationRail(
                extended: true,
                minExtendedWidth: 208,
                scrollable: true,
                trailingAtBottom: true,
                leading: Padding(
                  padding: const EdgeInsets.fromLTRB(16, 24, 24, 32),
                  child: Row(
                    children: [
                      const AppIcon(),
                      const SizedBox(width: 12),
                      Text(
                        'Astrave',
                        style: Theme.of(context).textTheme.headlineSmall
                            ?.copyWith(fontWeight: FontWeight.w700),
                      ),
                    ],
                  ),
                ),
                trailing: Padding(
                  padding: const EdgeInsets.all(16),
                  child: Row(children: actions),
                ),
                selectedIndex: page,
                onDestinationSelected: (i) => setState(() => page = i),
                destinations: [
                  for (var i = 0; i < titles.length; i++)
                    NavigationRailDestination(
                      icon: Icon(icons[i]),
                      label: Text(titles[i]),
                    ),
                ],
              ),
            Expanded(
              child: PageMotion(
                page: page,
                child: page == 0
                    ? panels
                    : SingleChildScrollView(
                        key: PageStorageKey(page),
                        padding: EdgeInsets.all(
                          wide ? Space.section : Space.medium,
                        ),
                        child: Align(
                          alignment: Alignment.topCenter,
                          child: ConstrainedBox(
                            constraints: const BoxConstraints(maxWidth: 1100),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.stretch,
                              children: [
                                heading,
                                const SizedBox(height: Space.large),
                                panels,
                              ],
                            ),
                          ),
                        ),
                      ),
              ),
            ),
          ],
        ),
      ),
      bottomNavigationBar: wide
          ? null
          : NavigationBar(
              selectedIndex: page,
              onDestinationSelected: (i) => setState(() => page = i),
              destinations: [
                for (var i = 0; i < titles.length; i++)
                  NavigationDestination(icon: Icon(icons[i]), label: titles[i]),
              ],
            ),
    );
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    session?.dispose();
    editor.dispose();
    logFilter.dispose();
    super.dispose();
  }
}

Future<String?> prompt(
  BuildContext context,
  String title,
  String label,
  String initial,
  LocalText t, {
  bool multiline = false,
}) {
  var value = initial;
  return showDialog<String>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(title),
      scrollable: true,
      content: SizedBox(
        width: 480,
        child: TextFormField(
          initialValue: initial,
          minLines: multiline ? 6 : 1,
          maxLines: multiline ? 15 : 1,
          autofocus: true,
          decoration: InputDecoration(labelText: label),
          onChanged: (v) => value = v,
          onFieldSubmitted: multiline
              ? null
              : (v) => Navigator.pop(context, v.trim()),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(t('Cancel', '取消')),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(context, value.trim()),
          child: Text(t('Continue', '继续')),
        ),
      ],
    ),
  );
}

Future<bool> confirm(BuildContext context, String title, LocalText t) async =>
    await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(title),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: Text(t('Cancel', '取消')),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text(t('Confirm', '确认')),
          ),
        ],
      ),
    ) ??
    false;
