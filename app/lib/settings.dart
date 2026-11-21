import 'package:material_ui/material_ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'design.dart';
import 'language.dart';
export 'language.dart' show AppLanguage;

enum AppTheme { system, light, dark, amoled, amoledMonet }

enum Palette {
  system(null),
  violet(Color(0xff6651a8)),
  ocean(Color(0xff23609a)),
  jade(Color(0xff16725c)),
  rose(Color(0xffa13a66)),
  ruby(Color(0xffbc2848)),
  amber(Color(0xffbb7800)),
  coral(Color(0xffdc684c)),
  moss(Color(0xff657a26)),
  teal(Color(0xff00868a)),
  sky(Color(0xff2699c6)),
  indigo(Color(0xff424ca0)),
  orchid(Color(0xffa249a5)),
  copper(Color(0xff9a573a)),
  slate(Color(0xff526879));

  const Palette(this.seed);
  final Color? seed;
}

enum NavGlass { solid, gaussian, aero, liquid }

class Appearance {
  const Appearance({
    this.language = AppLanguage.system,
    this.theme = AppTheme.system,
    this.palette = Palette.system,
    this.glass = NavGlass.gaussian,
  });
  final AppLanguage language;
  final AppTheme theme;
  final Palette palette;
  final NavGlass glass;

  Appearance copyWith({
    AppLanguage? language,
    AppTheme? theme,
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
        theme: read(AppTheme.values, 1, AppTheme.system),
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
          ? AppTheme.system
          : dark
          ? AppTheme.dark
          : AppTheme.light,
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
    required this.about,
    required this.updates,
  });
  final Appearance value;
  final LocalText text;
  final ValueChanged<Appearance> onChanged;
  final VoidCallback certificates;
  final VoidCallback about;
  final Widget updates;

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
      leading: _SettingsIcon(icon),
      title: Text(title),
      subtitle: Text(
        current is Palette &&
                current != Palette.system &&
                value.theme == AppTheme.amoledMonet
            ? '${t('Dynamic color', '系统动态色')} · ${options[current]}'
            : options[current]!,
      ),
      trailing: const Icon(Icons.expand_more_rounded),
      onTap: () async {
        final picked = await showDialog<T>(
          context: context,
          builder: (ctx) => SimpleDialog(
            title: Text(title),
            children: current is Palette
                ? [
                    Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 20),
                      child: Wrap(
                        spacing: 8,
                        runSpacing: 8,
                        children: [
                          for (final entry in options.entries)
                            ChoiceChip(
                              avatar: Icon(
                                entry.key == Palette.system
                                    ? Icons.auto_awesome_rounded
                                    : Icons.circle,
                                size: 18,
                                color: (entry.key as Palette).seed,
                              ),
                              label: Text(entry.value),
                              selected: entry.key == current,
                              onSelected: (_) => Navigator.pop(ctx, entry.key),
                            ),
                        ],
                      ),
                    ),
                  ]
                : [
                    for (final entry in options.entries)
                      ListTile(
                        selected: current == entry.key,
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
        for (final (index, row) in rows.indexed)
          Padding(
            padding: EdgeInsets.only(top: index == 0 ? 0 : 2),
            child: Material(
              color: Theme.of(context).colorScheme.surfaceContainerLow,
              borderRadius: BorderRadius.vertical(
                top: Radius.circular(index == 0 ? 22 : 5),
                bottom: Radius.circular(index == rows.length - 1 ? 22 : 5),
              ),
              clipBehavior: Clip.antiAlias,
              child: row,
            ),
          ),
      ],
    );
    final preferences = group(t('Preferences', '偏好设置'), [
      choice(Icons.translate_rounded, t('Language', '语言'), value.language, {
        AppLanguage.system: system,
        for (final lang in AppLanguage.values.skip(1)) lang: lang.label,
      }, (v) => onChanged(value.copyWith(language: v))),
      choice(Icons.contrast_rounded, t('Theme', '主题'), value.theme, {
        AppTheme.system: system,
        AppTheme.light: t('Light', '浅色'),
        AppTheme.dark: t('Dark', '深色'),
        AppTheme.amoled: 'AMOLED',
        AppTheme.amoledMonet: 'AMOLED Monet',
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
          Palette.ruby: t('Ruby', '红宝石'),
          Palette.amber: t('Amber', '琥珀'),
          Palette.coral: t('Coral', '珊瑚'),
          Palette.moss: t('Moss', '苔绿'),
          Palette.teal: t('Teal', '青绿'),
          Palette.sky: t('Sky', '天蓝'),
          Palette.indigo: t('Indigo', '靛蓝'),
          Palette.orchid: t('Orchid', '兰紫'),
          Palette.copper: t('Copper', '赤铜'),
          Palette.slate: t('Slate', '岩灰'),
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
    ]);
    final details = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        group(t('Security', '安全'), [
          ListTile(
            leading: const _SettingsIcon(Icons.verified_user_outlined),
            title: Text(t('Certificates', '证书')),
            subtitle: Text(
              t('DNS providers and issued certificates', 'DNS 服务商与已签发证书'),
            ),
            trailing: const Icon(Icons.chevron_right_rounded),
            onTap: certificates,
          ),
        ]),
        const SizedBox(height: Space.large),
        group('Astrave', [
          updates,
          ListTile(
            leading: const _SettingsIcon(Icons.info_outline_rounded),
            title: Text(t('About Astrave', '关于 Astrave')),
            onTap: about,
          ),
        ]),
      ],
    );
    return LayoutBuilder(
      builder: (context, box) => box.maxWidth >= 780
          ? Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(child: preferences),
                const SizedBox(width: Space.large),
                Expanded(child: details),
              ],
            )
          : Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                preferences,
                const SizedBox(height: Space.large),
                details,
              ],
            ),
    );
  }
}

class _SettingsIcon extends StatelessWidget {
  const _SettingsIcon(this.icon);
  final IconData icon;
  @override
  Widget build(BuildContext context) {
    final c = Theme.of(context).colorScheme;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: c.secondaryContainer,
        borderRadius: BorderRadius.circular(14),
      ),
      child: Padding(
        padding: const EdgeInsets.all(9),
        child: Icon(icon, size: 20, color: c.onSecondaryContainer),
      ),
    );
  }
}
