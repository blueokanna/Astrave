import 'package:material_ui/material_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'design.dart';

enum AppLanguage { system, en, zh }

enum Palette {
  system(null),
  violet(Color(0xff6651a8)),
  ocean(Color(0xff23609a)),
  jade(Color(0xff16725c)),
  rose(Color(0xffa13a66));

  const Palette(this.seed);
  final Color? seed;
}

enum NavGlass { solid, gaussian, aero, liquid }

class Appearance {
  const Appearance({
    this.language = AppLanguage.system,
    this.theme = ThemeMode.system,
    this.palette = Palette.system,
    this.glass = NavGlass.gaussian,
  });
  final AppLanguage language;
  final ThemeMode theme;
  final Palette palette;
  final NavGlass glass;

  Appearance copyWith({
    AppLanguage? language,
    ThemeMode? theme,
    Palette? palette,
    NavGlass? glass,
  }) => Appearance(
    language: language ?? this.language,
    theme: theme ?? this.theme,
    palette: palette ?? this.palette,
    glass: glass ?? this.glass,
  );

  static Future<Appearance> load(SharedPreferencesAsync store) async {
    final saved = await store.getStringList('appearance.v1');
    if (saved != null) {
      T read<T extends Enum>(List<T> values, int index, T fallback) =>
          values
              .where((v) => v.name == saved.elementAtOrNull(index))
              .firstOrNull ??
          fallback;
      return Appearance(
        language: read(AppLanguage.values, 0, AppLanguage.system),
        theme: read(ThemeMode.values, 1, ThemeMode.system),
        palette: read(Palette.values, 2, Palette.system),
        glass: read(NavGlass.values, 3, NavGlass.gaussian),
      );
    }
    final chinese = await store.getBool('chinese');
    final dark = await store.getBool('dark');
    return Appearance(
      language: chinese == null
          ? AppLanguage.system
          : chinese
          ? AppLanguage.zh
          : AppLanguage.en,
      theme: dark == null
          ? ThemeMode.system
          : dark
          ? ThemeMode.dark
          : ThemeMode.light,
    );
  }

  Future<void> save(SharedPreferencesAsync store) => store.setStringList(
    'appearance.v1',
    [language.name, theme.name, palette.name, glass.name],
  );
}

class SettingsPanel extends StatelessWidget {
  const SettingsPanel({
    super.key,
    required this.value,
    required this.text,
    required this.onChanged,
    required this.certificates,
  });
  final Appearance value;
  final LocalText text;
  final ValueChanged<Appearance> onChanged;
  final VoidCallback certificates;

  @override
  Widget build(BuildContext context) {
    final t = text;
    final system = t('System default', '跟随系统');
    Widget choice<T>(
      IconData icon,
      String title,
      T current,
      Map<T, String> options,
      ValueChanged<T> select,
    ) => ListTile(
      leading: Icon(icon),
      title: Text(title),
      subtitle: Text(options[current]!),
      trailing: const Icon(Icons.expand_more_rounded),
      onTap: () async {
        final picked = await showDialog<T>(
          context: context,
          builder: (ctx) => SimpleDialog(
            title: Text(title),
            children: [
              for (final entry in options.entries)
                ListTile(
                  selected: current == entry.key,
                  leading: switch (entry.key) {
                    Palette(seed: final seed?) => Icon(
                      Icons.circle,
                      color: seed,
                    ),
                    _ => null,
                  },
                  title: Text(entry.value),
                  trailing: current == entry.key
                      ? const Icon(Icons.check_rounded)
                      : null,
                  onTap: () => Navigator.pop(ctx, entry.key),
                ),
            ],
          ),
        );
        if (context.mounted && picked != null && picked != current) {
          select(picked);
        }
      },
    );
    Widget group(String title, List<Widget> rows) => Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 0, 12, 8),
          child: Text(
            title,
            style: Theme.of(context).textTheme.titleSmall
                ?.copyWith(color: Theme.of(context).colorScheme.primary),
          ),
        ),
        Card.filled(
          clipBehavior: Clip.antiAlias,
          child: Column(children: rows),
        ),
      ],
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        group(t('Preferences', '偏好设置'), [
          choice(Icons.translate_rounded, t('Language', '语言'), value.language, {
            AppLanguage.system: system,
            AppLanguage.en: 'English',
            AppLanguage.zh: '简体中文',
          }, (v) => onChanged(value.copyWith(language: v))),
          choice(Icons.contrast_rounded, t('Theme', '主题'), value.theme, {
            ThemeMode.system: system,
            ThemeMode.light: t('Light', '浅色'),
            ThemeMode.dark: t('Dark', '深色'),
          }, (v) => onChanged(value.copyWith(theme: v))),
          choice(
            Icons.palette_outlined,
            t('Color palette', '色彩方案'),
            value.palette,
            {
              Palette.system: t('Dynamic color', '系统动态色'),
              Palette.violet: t('Violet', '紫罗兰'),
              Palette.ocean: t('Ocean', '海蓝'),
              Palette.jade: t('Jade', '青玉'),
              Palette.rose: t('Rose', '玫瑰'),
            },
            (v) => onChanged(value.copyWith(palette: v)),
          ),
          choice(
            Icons.blur_on_rounded,
            t('Navigation capsule', '导航胶囊背景'),
            value.glass,
            {
              NavGlass.solid: t('Solid', '纯色'),
              NavGlass.gaussian: t('Gaussian blur', '高斯模糊'),
              NavGlass.aero: t('Aero Glass style', 'Aero Glass 风格'),
              NavGlass.liquid: t('Liquid Glass style', 'Liquid Glass 风格'),
            },
            (v) => onChanged(value.copyWith(glass: v)),
          ),
        ]),
        const SizedBox(height: Space.large),
        group(t('Security', '安全'), [
          ListTile(
            leading: const Icon(Icons.verified_user_outlined),
            title: Text(t('Certificates', '证书')),
            subtitle: Text(
              t('DNS providers and issued certificates', 'DNS 服务商与已签发证书'),
            ),
            trailing: const Icon(Icons.chevron_right_rounded),
            onTap: certificates,
          ),
        ]),
      ],
    );
  }
}
