// Run with the Dart SDK bundled with Flutter. The argument is a directory
// containing the two release binaries to measure. All sockets use loopback.
import 'dart:async';
import 'dart:collection';
import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

const host = '127.0.0.1';
const tunnelCount = 128;
final block = Uint8List.fromList(List.generate(65536, (i) => i & 255));

Future<List<int>> ports(int count) async {
  final sockets = <ServerSocket>[];
  try {
    for (var i = 0; i < count; ++i) {
      sockets.add(await ServerSocket.bind(host, 0));
    }
    return sockets.map((socket) => socket.port).toList();
  } finally {
    for (final socket in sockets) {
      await socket.close();
    }
  }
}

class Peer {
  Peer(this.socket) : chunks = StreamIterator(socket);
  final Socket socket;
  final StreamIterator<Uint8List> chunks;
  Future<void> roundTrip() async {
    socket.add(block);
    var offset = 0;
    while (offset < block.length) {
      if (!await chunks.moveNext()) throw StateError('Unexpected EOF');
      final data = chunks.current;
      for (final byte in data) {
        if (byte != block[offset++]) throw StateError('Corrupt echo');
      }
    }
  }
}

Future<Map<String, dynamic>> runTrial(Directory binaries) async {
  final work = await Directory.systemTemp.createTemp('astrave-benchmark-');
  final processes = <Process>[];
  final peers = <Peer>[];
  final accepted = <Socket>{};
  final echo = await ServerSocket.bind(host, 0);
  echo.listen((socket) {
    accepted.add(socket);
    void close() {
      accepted.remove(socket);
      socket.destroy();
    }

    socket.listen(
      socket.add,
      onDone: close,
      onError: (Object _) => close(),
      cancelOnError: true,
    );
    socket.done.catchError((Object _) {});
  });
  final http = HttpClient()..connectionTimeout = const Duration(seconds: 3);
  final secret = List.generate(16, (_) => Random.secure().nextInt(256));
  final password = base64Url.encode(secret);
  final authorization =
      'Basic ${base64.encode(utf8.encode('bench:$password'))}';
  Future<void> launch(String name, String config) async {
    final file = File('${work.path}/$name.toml');
    await file.writeAsString(config);
    final exe = '${binaries.path}/$name${Platform.isWindows ? '.exe' : ''}';
    final process = await Process.start(
      exe,
      ['-c', file.path],
      workingDirectory: work.path,
      environment: {
        'ASTRAVE_CONTROL_SOCKET': Platform.isWindows
            ? '\\\\.\\pipe\\astrave-bench-${work.uri.pathSegments.where((s) => s.isNotEmpty).last}'
            : '${work.path}/ipc.sock',
      },
    );
    processes.add(process);
    for (final stream in [process.stdout, process.stderr]) {
      unawaited(stream.drain<void>());
    }
  }

  try {
    final allocated = await ports(tunnelCount + 2);
    final control = allocated[0], dashboard = allocated[1];
    await launch(
      'astrave-server',
      'listen="$host:$control"\nproxyAddr="$host"\n[dashboard]\naddr="$host"\nport=$dashboard\nuser="bench"\npassword="$password"\n',
    );
    Future<Map> api(String path) async {
      final req = await http.getUrl(
        Uri.parse('http://$host:$dashboard/api/v1/$path'),
      );
      req.headers.set('Authorization', authorization);
      final res = await req.close();
      final body = await utf8.decoder.bind(res).join();
      if (res.statusCode != 200) throw StateError('HTTP ${res.statusCode}');
      return (jsonDecode(body) as Map)['data'] as Map;
    }

    final config = StringBuffer(
      'server="$host:$control"\ntransport.tls.enable=false\n',
    );
    for (var i = 0; i < tunnelCount; i++) {
      config.write(
        '[[tunnels]]\nname="bench-${i.toString().padLeft(3, '0')}"\nprotocol="tcp"\nservice="$host:${echo.port}"\nremotePort=${allocated[i + 2]}\n',
      );
    }
    await launch('astrave', config.toString());
    final ready = Stopwatch()..start();
    while (true) {
      try {
        if ((await api('tunnels?pageSize=1'))['total'] == tunnelCount) break;
      } on SocketException {
        /* server starting */
      }
      if (ready.elapsed > const Duration(seconds: 45))
        throw StateError('Tunnels did not become ready');
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
    for (var i = 0; i < 64; i++) {
      final peer = Peer(await Socket.connect(host, allocated[2]));
      peers.add(peer);
      await peer.roundTrip().timeout(const Duration(seconds: 10));
    }
    final result = <String, dynamic>{};
    if (Platform.isWindows) {
      for (final entry in processes.asMap().entries) {
        final memory = await Process.run('powershell', [
          '-NoProfile',
          '-Command',
          'Get-Process -Id ${entry.value.pid} | Select-Object WorkingSet64,PrivateMemorySize64 | ConvertTo-Json -Compress',
        ]);
        if (memory.exitCode != 0) throw StateError('Memory sampling failed');
        result[entry.key == 0 ? 'server' : 'client'] = jsonDecode(
          memory.stdout as String,
        );
      }
    }
    final timer = Stopwatch()..start();
    await Future.wait(
      peers.take(8).map((p) async {
        for (var i = 0; i < 512; i++) {
          await p.roundTrip();
        }
      }),
    ).timeout(const Duration(minutes: 2));
    result['duplexMiBPerSecond'] = 512000000 / timer.elapsedMicroseconds;
    for (final endpoint in [
      'system/info',
      'tunnels?page=4&pageSize=20&q=bench',
      'clients?pageSize=1',
    ]) {
      final samples = <int>[];
      for (var i = 0; i < 105; i++) {
        timer.reset();
        final page = await api(endpoint);
        if (i >= 5) samples.add(timer.elapsedMicroseconds);
        if (endpoint.startsWith('tunnels') &&
            (page['total'] != 128 || page['items'][0]['name'] != 'bench-060'))
          throw StateError('Wrong tunnel page');
      }
      samples.sort();
      result[endpoint] = {'p50_us': samples[49], 'p95_us': samples[94]};
    }
    return result;
  } finally {
    http.close(force: true);
    for (final p in peers) {
      p.socket.destroy();
    }
    for (final p in processes.reversed) {
      p.kill();
      await p.exitCode;
    }
    await echo.close();
    for (final socket in accepted.toList()) {
      socket.destroy();
    }
    await work.delete(recursive: true);
  }
}

Future<void> main(List<String> args) async {
  if (args.length == 1 && args.single == '--logs') {
    benchmarkLogs();
    return;
  }
  if (args.length != 1)
    throw ArgumentError('Usage: dart scripts/benchmark.dart RELEASE_DIRECTORY');
  final binaries = Directory(args.single).absolute;
  for (var i = 0; i < 3; i++) {
    stdout.writeln(jsonEncode(await runTrial(binaries)));
  }
}

void benchmarkLogs() {
  const limit = 500, appends = 100000, rebuilds = 10000, visible = 24;
  final lines = List.generate(limit, (i) => 'INFO tunnel $i connected');
  // Reuse input strings to isolate queue maintenance and visible row lookup.
  for (var trial = 0; trial < 4; trial++) {
    final list = List.of(lines);
    final queue = ListQueue.of(lines);
    final timer = Stopwatch()..start();
    for (var i = 0; i < appends; i++) {
      list.removeAt(0);
      list.add(lines[i % limit]);
    }
    final listUs = timer.elapsedMicroseconds;
    timer.reset();
    for (var i = 0; i < appends; i++) {
      queue.removeFirst();
      queue.add(lines[i % limit]);
    }
    final queueUs = timer.elapsedMicroseconds;
    if (list.join() != queue.join())
      throw StateError('Log queue contents differ');
    var copiedChecksum = 0, directChecksum = 0;
    timer.reset();
    for (var i = 0; i < rebuilds; i++) {
      final snapshot = [
        for (var j = limit - 1; j >= 0; j--) queue.elementAt(j),
      ];
      for (var j = 0; j < visible; j++) {
        copiedChecksum += snapshot[j].length;
      }
    }
    final copyUs = timer.elapsedMicroseconds;
    timer.reset();
    for (var i = 0; i < rebuilds; i++) {
      for (var j = 0; j < visible; j++) {
        directChecksum += queue.elementAt(limit - 1 - j).length;
      }
    }
    final directUs = timer.elapsedMicroseconds;
    if (copiedChecksum != directChecksum) throw StateError('Log rows differ');
    if (trial > 0) {
      stdout.writeln(
        jsonEncode({
          'listAppendUs': listUs,
          'queueAppendUs': queueUs,
          'copyAndReadUs': copyUs,
          'directReadUs': directUs,
        }),
      );
    }
  }
}
