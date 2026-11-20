import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import 'design.dart';
import 'session.dart';
import 'src/rust/api.dart' show Tunnel;

class Overview extends StatelessWidget {
  const Overview({
    super.key,
    required this.session,
    required this.text,
    required this.connect,
    required this.edit,
    this.heading,
    this.modify,
  });
  final Session session;
  final LocalText text;
  final VoidCallback connect, edit;
  final Widget? heading;
  final Future<void> Function(int? index, {bool remove})? modify;

  @override
  Widget build(BuildContext context) {
    final tunnels = session.profile.tunnels;
    final theme = Theme.of(context);
    return LayoutBuilder(
      builder: (context, box) {
        final inset = box.maxWidth < 600 ? Space.medium : Space.section;
        final side = math.max(inset, (box.maxWidth - 1100) / 2);
        return CustomScrollView(
          key: const PageStorageKey('overview-scroll'),
          slivers: [
            SliverPadding(
              padding: EdgeInsets.fromLTRB(side, inset, side, 8),
              sliver: SliverToBoxAdapter(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    if (heading != null) ...[
                      heading!,
                      const SizedBox(height: 18),
                    ],
                    _ConnectionCard(
                      session: session,
                      text: text,
                      connect: connect,
                      edit: edit,
                    ),
                    if (session.error case final message?)
                      Padding(
                        padding: const EdgeInsets.only(top: 12),
                        child: SelectableText(
                          message,
                          style: TextStyle(color: theme.colorScheme.error),
                        ),
                      ),
                    const SizedBox(height: 24),
                    Row(
                      children: [
                        Expanded(
                          child: Text(
                            '${text('Configured tunnels', '已配置隧道')}  ·  ${tunnels.length}',
                            style: theme.textTheme.titleLarge,
                          ),
                        ),
                        IconButton.filledTonal(
                          tooltip: text('Add tunnel', '添加隧道'),
                          onPressed: session.busy || modify == null
                              ? null
                              : () => modify!(null),
                          icon: const Icon(Icons.add_rounded),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
            SliverPadding(
              padding: EdgeInsets.fromLTRB(side, 0, side, inset),
              sliver: tunnels.isEmpty
                  ? SliverToBoxAdapter(
                      child: Card.outlined(
                        child: ListTile(
                          leading: const Icon(Icons.route_outlined),
                          title: Text(text('No tunnels configured', '尚未配置隧道')),
                        ),
                      ),
                    )
                  : SliverList.builder(
                      itemCount: tunnels.length,
                      itemBuilder: (context, index) {
                        final tunnel = tunnels[index];
                        final assigned =
                            session.status == 'Running' &&
                                !session.reloadPending
                            ? session.remotes[tunnel.name]
                            : null;
                        return _TunnelRow(
                          tunnel: tunnel,
                          assigned: assigned,
                          text: text,
                          edit: session.busy || modify == null
                              ? null
                              : (remove) => modify!(index, remove: remove),
                        );
                      },
                    ),
            ),
          ],
        );
      },
    );
  }
}

class _ConnectionCard extends StatelessWidget {
  const _ConnectionCard({
    required this.session,
    required this.text,
    required this.connect,
    required this.edit,
  });
  final Session session;
  final LocalText text;
  final VoidCallback connect, edit;

  @override
  Widget build(BuildContext context) {
    final p = session.profile;
    final t = text;
    final type = Theme.of(context).textTheme;
    final colors = Theme.of(context).colorScheme;
    final running = session.status == 'Running';
    final (status, icon) = switch (session.status) {
      'Running' => (t('Connected', '已连接'), Icons.link_rounded),
      'Starting' => (t('Connecting', '连接中'), Icons.sync_rounded),
      'Reconnecting' => (
        t('Reconnecting', '正在重新连接'),
        Icons.sync_problem_rounded,
      ),
      'Stopping' => (t('Disconnecting', '正在断开'), Icons.link_off_rounded),
      _ => (t('Disconnected', '未连接'), Icons.link_off_rounded),
    };
    const ink = Color(0xffedf5fc),
        muted = Color(0xffbdcbdc),
        mint = Color(0xff9ce9d3);
    final accent = running
        ? mint
        : session.active
        ? const Color(0xffffd49b)
        : muted;
    final metrics = [
      (
        Icons.swap_horiz_rounded,
        t('Transport', '传输协议'),
        p.transport.toUpperCase(),
      ),
      (
        Icons.lock_outline_rounded,
        t('Encryption', '加密'),
        p.tls || p.transport == 'quic' ? 'TLS' : t('Off', '关闭'),
      ),
      (
        Icons.call_split_rounded,
        t('Multiplexing', '多路复用'),
        p.transport == 'quic'
            ? 'QUIC'
            : p.multiplex
            ? 'yamux'
            : t('Off', '关闭'),
      ),
    ];
    return Card.filled(
      color: const Color(0xff182631),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(22, 18, 18, 18),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Wrap(
                        crossAxisAlignment: WrapCrossAlignment.center,
                        spacing: 8,
                        children: [
                          Icon(icon, color: accent, size: 20),
                          Text(
                            status,
                            style: type.titleSmall?.copyWith(color: accent),
                          ),
                        ],
                      ),
                    ),
                    IconButton(
                      tooltip: t('Edit profile', '编辑配置'),
                      onPressed: edit,
                      color: ink,
                      icon: const Icon(Icons.tune_rounded),
                    ),
                  ],
                ),
                LayoutBuilder(
                  builder: (context, box) => Row(
                    children: [
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              t('Profile server', '配置的服务器'),
                              style: type.labelMedium?.copyWith(color: muted),
                            ),
                            const SizedBox(height: 4),
                            SelectableText(
                              p.server,
                              style: type.headlineMedium?.copyWith(
                                fontFamily: 'monospace',
                                color: ink,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                          ],
                        ),
                      ),
                      if (box.maxWidth >=
                          540 * MediaQuery.textScalerOf(context).scale(12) / 12)
                        Container(
                          padding: const EdgeInsets.symmetric(horizontal: 18),
                          decoration: const BoxDecoration(
                            border: Border(
                              left: BorderSide(color: Color(0xff47616b)),
                            ),
                          ),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                t('Registered tunnels', '已注册隧道'),
                                style: type.labelMedium?.copyWith(color: muted),
                              ),
                              const SizedBox(height: 4),
                              Text(
                                '${running ? session.remotes.length : 0}',
                                style: type.headlineMedium?.copyWith(
                                  fontFamily: 'monospace',
                                  color: accent,
                                ),
                              ),
                            ],
                          ),
                        ),
                    ],
                  ),
                ),
                const SizedBox(height: 16),
                Wrap(
                  spacing: 10,
                  runSpacing: 6,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: [
                    FilledButton.icon(
                      style: FilledButton.styleFrom(
                        backgroundColor: mint,
                        foregroundColor: const Color(0xff10352d),
                      ),
                      onPressed: session.busy ? null : connect,
                      icon: Icon(
                        session.active
                            ? Icons.stop_rounded
                            : Icons.play_arrow_rounded,
                      ),
                      label: Text(
                        session.active
                            ? t('Disconnect', '断开连接')
                            : t('Connect', '连接'),
                      ),
                    ),
                    if (session.active && session.reloadPending)
                      TextButton(
                        style: TextButton.styleFrom(
                          foregroundColor: const Color(0xffffd49b),
                        ),
                        onPressed: edit,
                        child: Text(
                          t('Saved changes pending reload', '已保存更改，等待重载'),
                        ),
                      ),
                  ],
                ),
              ],
            ),
          ),
          ColoredBox(
            color: colors.surfaceContainerHigh,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 14),
              child: LayoutBuilder(
                builder: (_, box) {
                  final scale = MediaQuery.textScalerOf(context).scale(12) / 12;
                  final columns = (box.maxWidth / (105 * scale)).floor().clamp(
                    1,
                    3,
                  );
                  final width = (box.maxWidth - (columns - 1) * 8) / columns;
                  return Wrap(
                    spacing: 8,
                    runSpacing: 12,
                    children: [
                      for (final (icon, label, value) in metrics)
                        SizedBox(
                          width: width,
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                label,
                                style: type.labelMedium?.copyWith(
                                  color: colors.onSurfaceVariant,
                                ),
                              ),
                              const SizedBox(height: 3),
                              Row(
                                children: [
                                  Icon(icon, size: 16, color: colors.primary),
                                  const SizedBox(width: 6),
                                  Flexible(
                                    child: Text(value, style: type.titleSmall),
                                  ),
                                ],
                              ),
                            ],
                          ),
                        ),
                    ],
                  );
                },
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _TunnelRow extends StatelessWidget {
  const _TunnelRow({
    required this.tunnel,
    required this.assigned,
    required this.text,
    required this.edit,
  });
  final Tunnel tunnel;
  final String? assigned;
  final LocalText text;
  final Future<void> Function(bool remove)? edit;

  @override
  Widget build(BuildContext context) {
    final t = text;
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final live = assigned != null && assigned!.isNotEmpty;
    final endpoint = live ? assigned! : tunnel.publicEndpoint;
    final autoPort = endpoint.isEmpty || endpoint == '0';
    final portOnly = int.tryParse(endpoint) != null;
    final publicLabel = live
        ? t('Assigned', '已分配')
        : portOnly
        ? t('Public port', '映射端口')
        : t('Public endpoint', '映射端点');
    return Padding(
      padding: const EdgeInsets.only(top: 6),
      child: Card.outlined(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(16, 10, 6, 12),
          child: Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Wrap(
                      spacing: 9,
                      runSpacing: 3,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        DecoratedBox(
                          decoration: BoxDecoration(
                            color: colors.secondaryContainer,
                            borderRadius: BorderRadius.circular(6),
                          ),
                          child: Padding(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 7,
                              vertical: 3,
                            ),
                            child: Text(
                              tunnel.protocol.toUpperCase(),
                              style: theme.textTheme.labelSmall?.copyWith(
                                fontFamily: 'monospace',
                                color: colors.onSecondaryContainer,
                              ),
                            ),
                          ),
                        ),
                        Text(tunnel.name, style: theme.textTheme.titleMedium),
                      ],
                    ),
                    const SizedBox(height: 6),
                    Wrap(
                      spacing: 10,
                      runSpacing: 3,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        SelectableText(
                          '$publicLabel  ${autoPort ? t('Automatic', '自动') : endpoint}',
                          style: theme.textTheme.bodySmall,
                        ),
                        if (tunnel.service.isNotEmpty) ...[
                          Icon(
                            Icons.arrow_forward_rounded,
                            size: 14,
                            color: colors.onSurfaceVariant,
                          ),
                          SelectableText(
                            tunnel.service,
                            style: theme.textTheme.bodySmall?.copyWith(
                              fontFamily: 'monospace',
                            ),
                          ),
                        ],
                      ],
                    ),
                  ],
                ),
              ),
              ActionsMenu(
                tooltip: t('Tunnel actions', '隧道操作'),
                children: [
                  MenuItemButton(
                    onPressed: autoPort
                        ? null
                        : () =>
                              Clipboard.setData(ClipboardData(text: endpoint)),
                    child: Text(
                      portOnly
                          ? t('Copy port', '复制端口')
                          : t('Copy endpoint', '复制端点'),
                    ),
                  ),
                  MenuItemButton(
                    onPressed: edit == null ? null : () => edit!(false),
                    child: Text(t('Edit tunnel', '编辑隧道')),
                  ),
                  MenuItemButton(
                    onPressed: edit == null ? null : () => edit!(true),
                    child: Text(t('Remove tunnel', '删除隧道')),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}
