import 'dart:async';
import 'dart:collection';

import 'package:flutter/foundation.dart';

import 'src/rust/api.dart' as rust;

class Session extends ChangeNotifier {
  Session(this.profile);
  rust.Profile profile;
  String status = 'Stopped';
  DateTime? connectedSince;
  var remoteGeneration = BigInt.zero;
  Map<String, String> remotes = {};
  String? error;
  final logs = ListQueue<String>(500);
  Timer? _timer;
  Future<void>? _inFlight;
  bool _disposed = false;
  bool busy = false;
  bool reloadPending = false;
  bool get active => status != 'Stopped';

  void saved(rust.Profile next) {
    if (active && next.source != profile.source) reloadPending = true;
    profile = next;
  }

  Future<String> reload() async {
    final result = await rust.reloadClient(path: profile.path);
    reloadPending = false;
    return result;
  }

  Future<void> refresh() {
    if (_disposed) return Future.value();
    return _inFlight ??= _readState().whenComplete(() => _inFlight = null);
  }

  Future<void> _readState() async {
    try {
      final next = await rust.sessionState(remoteGeneration: remoteGeneration);
      final changed =
          status != next.status ||
          next.error != error ||
          next.lines.isNotEmpty ||
          next.remotes != null;
      remoteGeneration = next.remoteGeneration;
      remotes = next.remotes ?? remotes;
      if (next.status == 'Running') {
        connectedSince ??= DateTime.now();
      } else {
        connectedSince = null;
      }
      status = next.status;
      if (!active) reloadPending = false;
      error = next.error;
      for (final line in next.lines) {
        if (logs.length == 500) logs.removeFirst();
        logs.add(line);
      }
      if (changed && !_disposed) notifyListeners();
    } finally {
      _timer?.cancel();
      if (active && !_disposed) {
        _timer = Timer(const Duration(seconds: 1), () async {
          try {
            await refresh();
          } catch (e) {
            error = e.toString();
            if (!_disposed) notifyListeners();
          }
        });
      }
    }
  }

  Future<void> change(Future<void> Function() operation) async {
    if (busy) return;
    busy = true;
    notifyListeners();
    try {
      await operation();
      // A poll already in flight may describe the state before this action.
      // Read again even when that earlier poll failed.
      await _inFlight?.onError((_, _) {});
      await refresh();
    } finally {
      busy = false;
      if (!_disposed) notifyListeners();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    _timer?.cancel();
    super.dispose();
  }
}
