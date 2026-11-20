import 'package:material_ui/material_ui.dart';

import 'design.dart';
import 'main.dart' show confirm, prompt;
import 'src/rust/api.dart' as rust;

class Certificates extends StatefulWidget {
  const Certificates({
    super.key,
    required this.text,
    required this.profilePath,
  });
  final String profilePath;
  final LocalText text;
  @override
  State<Certificates> createState() => _CertificatesState();
}

class _CertificatesState extends State<Certificates> {
  late Future<rust.Inventory> items = rust.inventory();
  LocalText get t => widget.text;
  Future<void> change(Future<void> Function() action) async {
    try {
      await action();
      if (mounted) setState(() => items = rust.inventory());
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(e.toString())));
      }
    }
  }

  Future<void> provider([rust.ProviderInfo? old]) async {
    var name = old?.name ?? '',
        account = old?.account ?? '',
        secret = '',
        vendor = old?.vendor ?? 'cloudflare';
    final submit = await showDialog<bool>(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, update) => AlertDialog(
          title: Text(t('DNS provider', 'DNS 服务商')),
          content: SizedBox(
            width: 420,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  DropdownButtonFormField<String>(
                    initialValue: vendor,
                    decoration: InputDecoration(
                      labelText: t('Provider', '服务商'),
                    ),
                    items: [
                      for (final v in ['cloudflare', 'aliyun', 'tencent'])
                        DropdownMenuItem(value: v, child: Text(v)),
                    ],
                    onChanged: (v) => update(() => vendor = v!),
                  ),
                  const SizedBox(height: 12),
                  TextFormField(
                    initialValue: name,
                    decoration: InputDecoration(labelText: t('Name', '名称')),
                    onChanged: (v) => name = v,
                  ),
                  const SizedBox(height: 12),
                  if (vendor != 'cloudflare')
                    TextFormField(
                      initialValue: account,
                      decoration: const InputDecoration(
                        labelText: 'Access key ID',
                      ),
                      onChanged: (v) => account = v,
                    ),
                  TextFormField(
                    obscureText: true,
                    autocorrect: false,
                    enableSuggestions: false,
                    decoration: InputDecoration(
                      labelText: old == null
                          ? t('API secret', 'API 密钥')
                          : t('Replacement secret (optional)', '替换密钥（可选）'),
                    ),
                    onChanged: (v) => secret = v,
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
              onPressed: () => Navigator.pop(context, true),
              child: Text(t('Save', '保存')),
            ),
          ],
        ),
      ),
    );
    if (submit == true) {
      await change(
        () => rust.saveProvider(
          id: old?.id ?? '',
          name: name,
          vendor: vendor,
          account: account,
          secret: secret,
        ),
      );
    }
  }

  Future<void> issue(rust.ProviderInfo p) async {
    final domains = await prompt(
      context,
      t('Issue certificate', '签发证书'),
      t('Domain names, separated by commas', '域名，以逗号分隔'),
      '',
      t,
    );
    if (domains == null || !mounted) return;
    if (!await confirm(
          context,
          t(
            'Request a production certificate from Let’s Encrypt using ${p.name}?',
            '使用 ${p.name} 向 Let’s Encrypt 申请生产证书？',
          ),
          t,
        ) ||
        !mounted) {
      return;
    }
    final request = rust.issueCertificate(
      providerId: p.id,
      domains: domains.split(',').map((s) => s.trim()).toList(),
    );
    final result = await showDialog<String>(
      context: context,
      barrierDismissible: false,
      builder: (ctx) => PopScope(
        canPop: false,
        child: FutureBuilder<void>(
          future: request,
          builder: (ctx, state) {
            if (state.connectionState == ConnectionState.done) {
              WidgetsBinding.instance.addPostFrameCallback((_) {
                if (ctx.mounted) {
                  Navigator.pop(ctx, state.error?.toString() ?? '');
                }
              });
            }
            return AlertDialog(
              title: Text(t('Issuing certificate', '正在签发证书')),
              content: const LinearProgressIndicator(),
              actions: [
                TextButton(
                  onPressed: rust.cancelIssuance,
                  child: Text(t('Cancel request', '取消请求')),
                ),
              ],
            );
          },
        ),
      ),
    );
    if (!mounted) return;
    if (result != null && result.isNotEmpty) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(result)));
    }
    setState(() => items = rust.inventory());
  }

  @override
  Widget build(BuildContext context) => FutureBuilder<rust.Inventory>(
    future: items,
    builder: (context, state) {
      if (state.hasError) return SelectableText(state.error.toString());
      if (!state.hasData) return const LinearProgressIndicator();
      final data = state.requireData;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Wrap(
            alignment: WrapAlignment.spaceBetween,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Text(
                t('DNS providers', 'DNS 服务商'),
                style: Theme.of(context).textTheme.titleLarge,
              ),
              FilledButton.tonalIcon(
                onPressed: provider,
                icon: const Icon(Icons.add),
                label: Text(t('Add provider', '添加服务商')),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Card.filled(
            child: Column(
              children: [
                if (data.providers.isEmpty)
                  ListTile(
                    title: Text(t('No DNS providers', '尚无 DNS 服务商')),
                    subtitle: Text(
                      t(
                        'Add credentials to issue a certificate with DNS-01.',
                        '添加凭据后可通过 DNS-01 签发证书。',
                      ),
                    ),
                  ),
                for (final p in data.providers)
                  ListTile(
                    title: Text(p.name),
                    subtitle: Text(p.vendor),
                    onTap: () => provider(p),
                    trailing: Wrap(
                      children: [
                        IconButton(
                          tooltip: t('Issue certificate', '签发证书'),
                          onPressed: () => issue(p),
                          icon: const Icon(Icons.add_moderator_outlined),
                        ),
                        IconButton(
                          tooltip: t('Delete provider', '删除服务商'),
                          onPressed: () async {
                            if (await confirm(
                              context,
                              t('Delete ${p.name}?', '删除 ${p.name}？'),
                              t,
                            )) {
                              await change(() => rust.removeProvider(id: p.id));
                            }
                          },
                          icon: const Icon(Icons.delete_outline),
                        ),
                      ],
                    ),
                  ),
              ],
            ),
          ),
          const SizedBox(height: 24),
          Text(
            t('Issued certificates', '已签发证书'),
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: 12),
          Card.outlined(
            child: Column(
              children: [
                if (data.certificates.isEmpty)
                  ListTile(title: Text(t('No certificates issued', '尚未签发证书'))),
                for (final c in data.certificates)
                  ListTile(
                    title: SelectableText(c.domains.join(', ')),
                    subtitle: SelectableText(
                      '${t('Expires', '到期时间')}  ${c.expires}\n${c.chainPath}\n${c.keyPath}',
                    ),
                    trailing: IconButton(
                      tooltip: t('Delete certificate', '删除证书'),
                      onPressed: () async {
                        if (await confirm(
                          context,
                          t(
                            'Delete the certificate and private key?',
                            '删除证书及私钥？',
                          ),
                          t,
                        )) {
                          await change(
                            () => rust.removeCertificate(
                              id: c.id,
                              profilePath: widget.profilePath,
                            ),
                          );
                        }
                      },
                      icon: const Icon(Icons.delete_outline),
                    ),
                  ),
              ],
            ),
          ),
        ],
      );
    },
  );
}
