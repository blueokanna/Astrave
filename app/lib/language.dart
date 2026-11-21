import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

enum AppLanguage {
  system(null, ''),
  en(Locale('en'), 'English'),
  zh(Locale.fromSubtags(languageCode: 'zh', scriptCode: 'Hans'), '简体中文'),
  zhHant(Locale.fromSubtags(languageCode: 'zh', scriptCode: 'Hant'), '繁體中文'),
  de(Locale('de'), 'Deutsch'),
  fr(Locale('fr'), 'Français'),
  es(Locale('es'), 'Español'),
  ar(Locale('ar'), 'العربية'),
  pt(Locale('pt'), 'Português'),
  nl(Locale('nl'), 'Nederlands'),
  it(Locale('it'), 'Italiano'),
  ro(Locale('ro'), 'Română'),
  ja(Locale('ja'), '日本語'),
  ko(Locale('ko'), '한국어'),
  hi(Locale('hi'), 'हिन्दी'),
  ru(Locale('ru'), 'Русский');

  const AppLanguage(this.locale, this.label);
  final Locale? locale;
  final String label;

  AppLanguage resolve(List<Locale> preferred) {
    if (this != system) return this;
    for (final locale in preferred) {
      if (locale.languageCode == 'zh') {
        return locale.scriptCode == 'Hant' ||
                (locale.scriptCode == null &&
                    {'TW', 'HK', 'MO'}.contains(locale.countryCode))
            ? zhHant
            : zh;
      }
      for (final language in values) {
        if (language.locale?.languageCode == locale.languageCode) {
          return language;
        }
      }
    }
    return en;
  }
}

class Messages {
  const Messages(this.catalogs);
  final Map<AppLanguage, Map<String, String>> catalogs;

  static Future<Messages> load() async {
    final lines = const LineSplitter().convert(
      await rootBundle.loadString('assets/messages.tsv'),
    );
    final languages = lines.first
        .split('\t')
        .skip(1)
        .map(AppLanguage.values.byName)
        .toList();
    final catalogs = {for (final lang in languages) lang: <String, String>{}};
    for (final line in lines.skip(1)) {
      final cells = line.split('\t');
      if (cells.length != languages.length + 1 || cells.any((s) => s.isEmpty)) {
        throw FormatException('Incomplete translation for ${cells.first}');
      }
      for (final (i, language) in languages.indexed) {
        catalogs[language]![cells.first] = cells[i + 1];
      }
    }
    return Messages(catalogs);
  }

  String text(AppLanguage language, String en, String zh) =>
      language == AppLanguage.zh ? zh : catalogs[language]?[en] ?? en;
}
