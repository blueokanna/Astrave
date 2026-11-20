import 'package:material_ui/material_ui.dart';

import 'design.dart';
import 'session.dart';

class ActivityPanel extends StatelessWidget {
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
  Widget build(BuildContext context) => Column(
    children: [
      Row(
        children: [
          Expanded(
            child: TextField(
              controller: filter,
              decoration: InputDecoration(
                labelText: text('Filter logs', '筛选日志'),
              ),
            ),
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
          final matches = query.isEmpty
              ? null
              : [
                  for (var i = 0; i < logs.length; i++)
                    if (logs.newest(i).toLowerCase().contains(query))
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
            child: ListView.builder(
              itemCount: count,
              itemBuilder: (context, index) => SelectableText(
                matches?[index] ?? logs.newest(index),
                style: style,
              ),
            ),
          );
        },
      ),
    ],
  );
}
