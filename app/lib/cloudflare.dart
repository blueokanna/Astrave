import 'dart:convert';

import 'package:material_ui/material_ui.dart';

import 'design.dart';

Future<String?> cloudflareProfile(BuildContext context, LocalText t) async {
  var token = '';
  var id = '';
  var protocol = 'auto';
  return showDialog<String>(
    context: context,
    builder: (context) => StatefulBuilder(
      builder: (context, update) => AlertDialog(
        title: const Text('Cloudflare Tunnel'),
        content: SizedBox(
          width: 480,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  t(
                    'Connect directly with Astrave. Manage public hostnames and origin services in your Cloudflare tunnel dashboard.',
                    '由 Astrave 直接连接。在 Cloudflare 隧道控制台管理公共域名和源站服务。',
                  ),
                ),
                const SizedBox(height: 16),
                TextField(
                  obscureText: true,
                  autocorrect: false,
                  enableSuggestions: false,
                  decoration: const InputDecoration(labelText: 'Tunnel Token'),
                  onChanged: (value) => update(() => token = value),
                ),
                const SizedBox(height: 12),
                TextField(
                  onChanged: (value) => id = value,
                  autocorrect: false,
                  decoration: InputDecoration(
                    labelText: t('Tunnel ID (optional)', 'Tunnel ID（可选）'),
                  ),
                ),
                const SizedBox(height: 12),
                DropdownMenu<String>(
                  initialSelection: protocol,
                  expandedInsets: EdgeInsets.zero,
                  label: Text(t('Transport', '传输协议')),
                  dropdownMenuEntries: [
                    DropdownMenuEntry(
                      value: 'auto',
                      label: t('Automatic', '自动'),
                    ),
                    const DropdownMenuEntry(value: 'quic', label: 'QUIC'),
                    const DropdownMenuEntry(value: 'http2', label: 'HTTP/2'),
                  ],
                  onSelected: (value) {
                    if (value != null) protocol = value;
                  },
                ),
                const SizedBox(height: 16),
                Text(
                  t(
                    'This replaces the editor contents. Review and save the profile, then connect from Overview.',
                    '此操作会替换编辑器内容。检查并保存配置后，在概览页点击连接。',
                  ),
                ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text(t('Cancel', '取消')),
          ),
          FilledButton(
            onPressed: token.trim().isEmpty
                ? null
                : () => Navigator.pop(
                    context,
                    '[cloudflare]\n'
                    'token = ${jsonEncode(token.trim())}\n'
                    '${id.trim().isEmpty ? '' : 'tunnelId = ${jsonEncode(id.trim())}\n'}'
                    'protocol = "$protocol"\n'
                    'connections = 4\n',
                  ),
            child: Text(t('Use this configuration', '使用此配置')),
          ),
        ],
      ),
    ),
  );
}
