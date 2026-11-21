import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'benchmark.dart' show host, ports;

const peerCount = 256;
const packetSize = 65535;

Future<Map<String, Object?>> trial(Directory binaries) async {
  final temp = await Directory.systemTemp.createTemp('astrave-udp-');
  final processes = <Process>[];
  final peers = <RawDatagramSocket>[];
  final address = InternetAddress.loopbackIPv4;
  final echo = await RawDatagramSocket.bind(address, 0);
  echo.listen((event) {
    if (event != RawSocketEvent.read) return;
    for (var d = echo.receive(); d != null; d = echo.receive()) {
      echo.send(d.data, d.address, d.port);
    }
  });
  Future<void> launch(String name, String config) async {
    final cfg = File('${temp.path}/$name.toml');
    await cfg.writeAsString(config);
    final p = await Process.start(
      '${binaries.path}/$name${Platform.isWindows ? '.exe' : ''}',
      ['-c', cfg.path],
      workingDirectory: temp.path,
      environment: {
        'ASTRAVE_CONTROL_SOCKET': Platform.isWindows
            ? '\\\\.\\pipe\\astrave-udp-$pid'
            : '${temp.path}/control.sock',
      },
    );
    processes.add(p);
    unawaited(p.stdout.drain<void>());
    unawaited(p.stderr.drain<void>());
  }

  try {
    final control = (await ports(1)).single;
    final reserve = await RawDatagramSocket.bind(address, 0);
    final mapped = reserve.port;
    reserve.close();
    await launch(
      'astrave-server',
      'listen="$host:$control"\nproxyAddr="$host"\nudpPacketSize=$packetSize\n',
    );
    await launch(
      'astrave',
      'server="$host:$control"\nudpPacketSize=$packetSize\ntransport.tls.enable=false\n[[tunnels]]\nname="echo"\nprotocol="udp"\nservice="$host:${echo.port}"\nremotePort=$mapped\n',
    );
    final replies = <Completer<void>?>[];
    for (var i = 0; i < peerCount; ++i) {
      final id = i;
      final socket = await RawDatagramSocket.bind(address, 0);
      peers.add(socket);
      replies.add(null);
      socket.listen((event) {
        if (event != RawSocketEvent.read) return;
        for (var d = socket.receive(); d != null; d = socket.receive()) {
          final reply = replies[id];
          if (reply == null || reply.isCompleted) continue;
          if (d.port != mapped ||
              d.data.length != 256 ||
              d.data.any((b) => b != id)) {
            reply.completeError(StateError('Incorrect echo for peer $id'));
          } else {
            reply.complete();
          }
        }
      });
      final done = Completer<void>();
      final payload = Uint8List(256)..fillRange(0, 256, id);
      replies[id] = done;
      final deadline = Stopwatch()..start();
      while (!done.isCompleted) {
        if (deadline.elapsed > const Duration(seconds: 20))
          throw StateError('No echo for peer $id');
        socket.send(payload, address, mapped);
        await done.future.timeout(
          const Duration(milliseconds: 250),
          onTimeout: () {},
        );
      }
    }
    final result = <String, Object?>{
      'peers': peerCount,
      'configuredDatagramBytes': packetSize,
    };
    if (Platform.isWindows) {
      final p = await Process.run('powershell', [
        '-NoProfile',
        '-Command',
        'Get-Process -Id ${processes.last.pid} | Select-Object WorkingSet64,PrivateMemorySize64 | ConvertTo-Json -Compress',
      ]);
      if (p.exitCode != 0) throw StateError('Client memory sampling failed');
      result['client'] = jsonDecode(p.stdout as String);
    }
    return result;
  } finally {
    for (final peer in peers) {
      peer.close();
    }
    echo.close();
    for (final p in processes.reversed) {
      p.kill();
      await p.exitCode;
    }
    await temp.delete(recursive: true);
  }
}

Future<void> main(List<String> args) async {
  if (args.length != 2)
    throw ArgumentError('Expected baseline and candidate binary directories');
  for (final index in [0, 1, 1, 0, 0, 1]) {
    stdout.writeln(
      jsonEncode({
        'build': index == 0 ? 'before' : 'after',
        ...await trial(Directory(args[index]).absolute),
      }),
    );
  }
}
