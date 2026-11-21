import 'package:material_ui/material_ui.dart';

import 'design.dart';
import 'session.dart';

enum _Level {
  info(Icons.info_outline),
  warn(Icons.warning_amber_rounded),
  error(Icons.error_outline),
  event(Icons.notes_rounded);

  const _Level(this.icon);
  final IconData icon;
  static (_Level, String) parse(String line) {
    final match = _prefix.firstMatch(line);
    if (match == null) return (_Level.event, line);
    return (
      _Level.values.byName(match[1]!.toLowerCase()),
      line.substring(match.end),
    );
  }

  static final _prefix = RegExp(r'^(INFO|WARN|ERROR)(?:\s+|$)');
}

class ActivityPanel extends StatefulWidget {
  const ActivityPanel({
    super.key,
    required this.logs,
    required this.filter,
    required this.text,
  });

  final SessionLog logs;
  final TextEditingController filter;
  final LocalText text;

  @override
  State<ActivityPanel> createState() => _ActivityPanelState();
}

class _ActivityPanelState extends State<ActivityPanel> {
  _Level? selected;
  SessionLog get logs => widget.logs;
  TextEditingController get filter => widget.filter;
  LocalText get text => widget.text;

  @override
  Widget build(BuildContext context) => Column(
    children: [
      Row(
        children: [
          Expanded(
            child: TextField(
              controller: filter,
              decoration: InputDecoration(
                prefixIcon: const Icon(Icons.search_rounded),
                labelText: text('Filter logs', '筛选日志'),
              ),
            ),
          ),
          PopupMenuButton<int>(
            tooltip: text('Severity', '日志级别'),
            icon: Icon(selected?.icon ?? Icons.filter_list_rounded),
            initialValue: selected?.index ?? -1,
            onSelected: (v) =>
                setState(() => selected = v < 0 ? null : _Level.values[v]),
            itemBuilder: (_) => [
              PopupMenuItem(value: -1, child: Text(text('All levels', '全部级别'))),
              for (final level in [_Level.info, _Level.warn, _Level.error])
                PopupMenuItem(
                  value: level.index,
                  child: Text(level.name.toUpperCase()),
                ),
            ],
          ),
          IconButton(
            tooltip: text('Clear logs', '清空日志'),
            onPressed: logs.clear,
            icon: const Icon(Icons.clear_all),
          ),
        ],
      ),
      const SizedBox(height: 12),
      ListenableBuilder(
        listenable: Listenable.merge([logs, filter]),
        builder: (context, _) {
          final query = filter.text.toLowerCase();
          final matches = query.isEmpty && selected == null
              ? null
              : [
                  for (var i = 0; i < logs.length; i++)
                    if (logs.newest(i).toLowerCase().contains(query) &&
                        (selected == null ||
                            _Level.parse(logs.newest(i)).$1 == selected))
                      logs.newest(i),
                ];
          final count = matches?.length ?? logs.length;
          if (count == 0) {
            return ListTile(
              leading: const Icon(Icons.subject),
              title: Text(text('No matching events', '暂无匹配事件')),
            );
          }
          final size = MediaQuery.sizeOf(context);
          final style = Theme.of(context).textTheme.bodyMedium
              ?.copyWith(fontFamily: 'monospace', height: 1.6);
          return SizedBox(
            height: (size.height - (size.width >= 800 ? 150 : 215)).clamp(
              140.0,
              1600.0,
            ),
            child: ListView.separated(
              separatorBuilder: (_, _) => const Divider(height: 1),
              itemCount: count,
              itemBuilder: (context, index) {
                final (level, message) = _Level.parse(
                  matches?[index] ?? logs.newest(index),
                );
                final c = Theme.of(context).colorScheme;
                final (bg, fg) = switch (level) {
                  _Level.error => (c.errorContainer, c.onErrorContainer),
                  _Level.warn =>
                    Theme.of(context).brightness == Brightness.dark
                        ? (const Color(0xff523b00), const Color(0xffffdfa2))
                        : (const Color(0xffffe3a7), const Color(0xff513900)),
                  _Level.info => (c.secondaryContainer, c.onSecondaryContainer),
                  _Level.event => (c.surfaceContainerHigh, c.onSurfaceVariant),
                };
                return Padding(
                  padding: const EdgeInsets.symmetric(vertical: 9),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      DecoratedBox(
                        decoration: BoxDecoration(
                          color: bg,
                          borderRadius: BorderRadius.circular(8),
                        ),
                        child: Padding(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 7,
                            vertical: 4,
                          ),
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Icon(level.icon, color: fg, size: 15),
                              if (level != _Level.event) ...[
                                const SizedBox(width: 4),
                                Text(
                                  level.name.toUpperCase(),
                                  style: Theme.of(context).textTheme.labelSmall
                                      ?.copyWith(
                                        color: fg,
                                        fontWeight: FontWeight.bold,
                                      ),
                                ),
                              ],
                            ],
                          ),
                        ),
                      ),
                      const SizedBox(width: 10),
                      Expanded(child: SelectableText(message, style: style)),
                    ],
                  ),
                );
              },
            ),
          );
        },
      ),
    ],
  );
}
