import 'dart:async';
import 'dart:collection';

import 'package:flutter/foundation.dart';

import 'src/rust/api.dart' as rust;

class SessionLog extends ChangeNotifier {
  static const limit = 500;
  final _lines = ListQueue<String>();

  int get length => _lines.length;
  String newest(int index) => _lines.elementAt(length - 1 - index);

  void append(List<String> lines) {
    if (lines.isEmpty) return;
    for (final line in lines) {
      if (length == limit) _lines.removeFirst();
      _lines.addLast(line);
    }
    notifyListeners();
  }

  void clear() {
    if (_lines.isEmpty) return;
    _lines.clear();
    notifyListeners();
  }
}

class Session extends ChangeNotifier {
  Session(this.profile);
  rust.Profile profile;
  String status = 'Stopped';
  DateTime? connectedSince;
  var remoteGeneration = BigInt.zero;
  Map<String, String> remotes = {};
  String? error;
  final logs = SessionLog();
  Timer? _timer;
  Future<void>? _inFlight;
  bool _disposed = false;
  bool _pollingEnabled = true;
  bool busy = false;
  bool reloadPending = false;
  bool get active => status != 'Stopped';

  void setPollingEnabled(bool enabled) {
    if (_pollingEnabled == enabled) return;
    _pollingEnabled = enabled;
    if (enabled) {
      unawaited(_poll());
    } else {
      _timer?.cancel();
    }
  }

  Future<void> _poll() async {
    try {
      await refresh();
    } catch (e) {
      error = e.toString();
      if (!_disposed) notifyListeners();
    }
  }

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
      if (_disposed) return;
      final changed =
          status != next.status || next.error != error || next.remotes != null;
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
      logs.append(next.lines);
      if (changed) notifyListeners();
    } finally {
      _timer?.cancel();
      if (active && _pollingEnabled && !_disposed) {
        _timer = Timer(const Duration(seconds: 1), _poll);
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
    logs.dispose();
    super.dispose();
  }
}
