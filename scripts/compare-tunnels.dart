// Supply native client and server directories. All three endpoints bind to loopback.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'benchmark.dart' show Peer, block, host, ports;

Future<void> trial(String product, Directory bin) async {
  final temp = await Directory.systemTemp.createTemp('astrave-compare-');
  final echo = await ServerSocket.bind(host, 0);
  final workers = <Process>[];
  final accepted = <Socket>{};
  echo.listen((socket) {
    accepted.add(socket);
    socket.listen(
      socket.add,
      onDone: () {
        accepted.remove(socket);
        socket.destroy();
      },
      onError: (Object e) {
        socket.destroy();
      },
    );
  });
  final peers = <Peer>[];
  final [control, mapped] = await ports(2);
  final frp = product == 'frp';
  final server = frp ? 'frps' : '$product-server';
  final client = frp ? 'frpc' : product;
  Future<Process> launch(String exe, String config) async {
    final file = File('${temp.path}/$exe.toml');
    await file.writeAsString(config);
    final p = await Process.start(
      '${bin.path}/$exe${Platform.isWindows ? '.exe' : ''}',
      ['-c', file.path],
      workingDirectory: temp.path,
      environment: {
        'ASTRAVE_CONTROL_SOCKET': Platform.isWindows
            ? '\\\\.\\pipe\\astrave-compare-$pid'
            : '${temp.path}/control.sock',
      },
    );
    workers.add(p);
    for (final stream in [p.stdout, p.stderr]) {
      unawaited(stream.drain<void>());
    }
    return p;
  }

  Future<Peer> connect() async {
    final deadline = Stopwatch()..start();
    while (true) {
      Socket? socket;
      try {
        socket = await Socket.connect(
          host,
          mapped,
          timeout: const Duration(seconds: 2),
        );
        final p = Peer(socket);
        await p.roundTrip().timeout(const Duration(seconds: 3));
        return p;
      } catch (e) {
        socket?.destroy();
        if (deadline.elapsed > const Duration(seconds: 45)) rethrow;
        await Future<void>.delayed(const Duration(milliseconds: 100));
      }
    }
  }

  final gateway = frp
      ? 'bindAddr="$host"\nbindPort=$control\nproxyBindAddr="$host"\ntransport.tcpMux=true\nauth.token="bench"\n'
      : 'listen="$host:$control"\nproxyAddr="$host"\ntransport.tcpMux=true\nauth.token="bench"\n';
  try {
    final first = await launch(server, gateway);
    await launch(
      client,
      frp
          ? 'serverAddr="$host"\nserverPort=$control\nloginFailExit=false\nauth.token="bench"\ntransport.tcpMux=true\ntransport.tls.enable=false\n[[proxies]]\nname="echo"\ntype="tcp"\nlocalIP="$host"\nlocalPort=${echo.port}\nremotePort=$mapped\n'
          : 'server="$host:$control"\nauth.token="bench"\ntransport.tcpMux=true\ntransport.tls.enable=false\n[[tunnels]]\nname="echo"\nprotocol="tcp"\nservice="$host:${echo.port}"\nremotePort=$mapped\n',
    );
    for (var i = 0; i < 8; i++) {
      peers.add(await connect());
    }
    final timer = Stopwatch()..start();
    const count = 128;
    await Future.wait(
      peers.map((p) async {
        for (var i = 0; i < count; i++) {
          await p.roundTrip();
        }
      }),
    ).timeout(const Duration(minutes: 2));
    timer.stop();
    final bytes = 2 * count * peers.length * block.length;
    for (final peer in peers) {
      peer.socket.destroy();
    }
    first.kill(ProcessSignal.sigkill);
    await first.exitCode;
    final recovery = Stopwatch()..start();
    await launch(server, gateway);
    peers.add(await connect());
    recovery.stop();
    stdout.writeln(
      jsonEncode({
        'product': product,
        'verifiedDuplexBytes': bytes,
        'elapsedUs': timer.elapsedMicroseconds,
        'restartToVerifiedEchoMs': recovery.elapsedMilliseconds,
      }),
    );
  } finally {
    for (final peer in peers) {
      peer.socket.destroy();
    }
    for (final p in workers.reversed) {
      p.kill();
      await p.exitCode;
    }
    await echo.close();
    for (final s in accepted.toList()) {
      s.destroy();
    }
    await temp.delete(recursive: true);
  }
}

Future<void> main(List<String> args) async {
  if (args.isEmpty) throw ArgumentError('Pass astrave=DIR orbien=DIR frp=DIR');
  final tools = <String, Directory>{};
  for (final arg in args) {
    final cut = arg.indexOf('=');
    final name = arg.substring(0, cut);
    if (!['astrave', 'orbien', 'frp'].contains(name)) throw ArgumentError(name);
    tools[name] = Directory(arg.substring(cut + 1)).absolute;
  }
  final names = tools.keys.toList();
  for (var round = 0; round < 3; round++) {
    for (var i = 0; i < names.length; i++) {
      final name = names[(round + i) % names.length];
      await trial(name, tools[name]!);
    }
  }
}
