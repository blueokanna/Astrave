import 'dart:async';
import 'dart:convert';
import 'dart:ffi' show Abi;
import 'dart:io';
import 'dart:typed_data';

import 'package:material_ui/material_ui.dart';
import 'package:crypto/crypto.dart';
import 'package:path_provider/path_provider.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:pub_semver/pub_semver.dart';
import 'package:url_launcher/url_launcher.dart';

import 'design.dart';
import 'android_update.dart';

const _repo = 'blueokanna/Astrave';

class PublishedRelease {
  const PublishedRelease(this.version, this.url, this.sha, this.size);
  final Version version;
  final Uri url;
  final String? sha;
  final int size;

  factory PublishedRelease.fromJson(Map<String, dynamic> json, Abi abi) {
    final tag = json['tag_name'] as String;
    if (!RegExp(r'^v\d+\.\d+\.\d+$').hasMatch(tag) ||
        json['draft'] != false ||
        json['prerelease'] != false) {
      throw const FormatException('Expected a published stable release');
    }
    final version = Version.parse(tag.substring(1));
    final suffix = switch (abi) {
      Abi.androidArm => 'android_armeabi-v7a.apk',
      Abi.androidArm64 => 'android_arm64-v8a.apk',
      Abi.androidX64 => 'android_x86_64.apk',
      Abi.linuxArm64 => 'linux_arm64.deb',
      Abi.linuxX64 => 'linux_amd64.deb',
      Abi.macosArm64 => 'darwin_arm64.dmg',
      Abi.macosX64 => 'darwin_amd64.dmg',
      Abi.windowsArm64 => 'windows_arm64.zip',
      Abi.windowsX64 => 'windows_amd64.zip',
      _ => null,
    };
    final prefix =
        {Abi.androidArm, Abi.androidArm64, Abi.androidX64}.contains(abi)
        ? 'astrave'
        : 'astrave-desktop';
    final asset = '${prefix}_${version}_$suffix';
    final match = (json['assets'] as List)
        .whereType<Map>()
        .where((a) => a['name'] == asset && a['state'] == 'uploaded')
        .firstOrNull;
    final digest = match?['digest'];
    final size = match?['size'];
    if (match != null &&
        (digest is! String ||
            !RegExp(r'^sha256:[a-f0-9]{64}$').hasMatch(digest) ||
            size is! int ||
            size <= 0 ||
            size > 1024 * 1024 * 1024)) {
      throw const FormatException('Invalid GitHub asset digest or size');
    }
    return PublishedRelease(
      version,
      Uri.https(
        'github.com',
        suffix != null && match != null
            ? '/$_repo/releases/download/$tag/$asset'
            : '/$_repo/releases/tag/$tag',
      ),
      (digest as String?)?.substring(7),
      size as int? ?? 0,
    );
  }
}

class ReleaseUpdates extends ChangeNotifier with WidgetsBindingObserver {
  ReleaseUpdates({HttpClient? client})
    : _client =
          client ??
          (HttpClient()..connectionTimeout = const Duration(seconds: 8));
  final HttpClient _client;
  static const interval = Duration(minutes: 15);
  AppLifecycleState? _state;
  Timer? _timer;
  bool checking = false, _closed = false;
  bool downloading = false;
  String? error, _etag;
  Version? installed;
  PublishedRelease? latest;
  DateTime _nextCheck = DateTime.fromMillisecondsSinceEpoch(0);

  void start() {
    WidgetsBinding.instance.addObserver(this);
    didChangeAppLifecycleState(
      WidgetsBinding.instance.lifecycleState ?? AppLifecycleState.resumed,
    );
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _state = state;
    _timer?.cancel();
    if (state == AppLifecycleState.resumed) _schedule();
  }

  void _schedule() {
    if (_closed || checking || _state != AppLifecycleState.resumed) return;
    _timer?.cancel();
    _timer = Timer(_nextCheck.difference(DateTime.now()), check);
  }

  Future<void> check() async {
    if (_closed || checking || downloading) return;
    checking = true;
    error = null;
    notifyListeners();
    HttpClientRequest? request;
    try {
      installed ??= Version.parse((await PackageInfo.fromPlatform()).version);
      request = await _client.getUrl(
        Uri.https('api.github.com', '/repos/$_repo/releases/latest'),
      );
      request.followRedirects = false;
      request.headers.set(HttpHeaders.userAgentHeader, 'Astrave/$installed');
      if (_etag case final tag?) {
        request.headers.set(HttpHeaders.ifNoneMatchHeader, tag);
      }
      final response = await request.close().timeout(
        const Duration(seconds: 12),
      );
      if (response.statusCode == HttpStatus.notModified) {
        await response.drain<void>().timeout(const Duration(seconds: 5));
      } else {
        if (response.statusCode != HttpStatus.ok) {
          throw HttpException('GitHub HTTP ${response.statusCode}');
        }
        final buffer = BytesBuilder(copy: false);
        await response
            .forEach((bytes) {
              if (buffer.length + bytes.length > 1024 * 1024) {
                throw const FormatException('GitHub response too large');
              }
              buffer.add(bytes);
            })
            .timeout(const Duration(seconds: 15));
        latest = PublishedRelease.fromJson(
          jsonDecode(utf8.decode(buffer.takeBytes())) as Map<String, dynamic>,
          Abi.current(),
        );
        _etag = response.headers.value(HttpHeaders.etagHeader);
      }
    } catch (e) {
      request?.abort();
      error = e.toString();
    } finally {
      checking = false;
      _nextCheck = DateTime.now().add(interval);
      if (!_closed) {
        notifyListeners();
        _schedule();
      }
    }
  }

  Future<void> download() async {
    final release = latest, current = installed;
    if (_closed ||
        downloading ||
        checking ||
        release == null ||
        current == null ||
        release.version <= current) {
      return;
    }
    downloading = true;
    error = null;
    notifyListeners();
    Directory? staging;
    try {
      if (release.sha == null) {
        if (!await launchUrl(
          release.url,
          mode: LaunchMode.externalApplication,
        )) {
          throw StateError(release.url.toString());
        }
        return;
      }
      final cache = await getTemporaryDirectory();
      final dir = await Directory('${cache.path}/updates')
          .create(recursive: true);
      staging = await dir.createTemp('release-');
      final file = File('${staging.path}/${release.url.pathSegments.last}');
      await release.downloadTo(_client, file);
      if (_closed) return;
      if (Platform.isAndroid) {
        openAndroidUpdate(file.path);
      } else if (!await launchUrl(
        file.uri,
        mode: LaunchMode.externalApplication,
      )) {
        throw StateError('Could not open ${file.path}');
      }
      staging = null; // The OS installer can still be reading the file.
    } catch (e) {
      error = e.toString();
    } finally {
      try {
        if (staging != null) await staging.delete(recursive: true);
      } on FileSystemException catch (cleanupError) {
        error ??= cleanupError.toString();
      } finally {
        downloading = false;
        if (!_closed) {
          notifyListeners();
          _schedule();
        }
      }
    }
  }

  @override
  void dispose() {
    _closed = true;
    WidgetsBinding.instance.removeObserver(this);
    _timer?.cancel();
    _client.close(force: true);
    super.dispose();
  }
}

extension ReleaseAssetDownload on PublishedRelease {
  Future<void> downloadTo(HttpClient client, File file) async {
    final want = sha;
    if (want == null) throw StateError('Release has no matching asset');
    final request = await client.getUrl(url);
    request.followRedirects = false;
    var response = await request.close().timeout(const Duration(seconds: 20));
    if (response.isRedirect) {
      final location = response.headers.value(HttpHeaders.locationHeader);
      final next = location == null ? null : Uri.tryParse(location);
      if (next == null ||
          next.scheme != 'https' ||
          next.hasPort && next.port != 443 ||
          next.userInfo.isNotEmpty ||
          !{
            'release-assets.githubusercontent.com',
            'objects.githubusercontent.com',
          }.contains(next.host)) {
        throw const FormatException('Unexpected GitHub asset redirect');
      }
      await response.drain<void>().timeout(const Duration(seconds: 8));
      final download = await client.getUrl(next);
      download.followRedirects = false;
      response = await download.close().timeout(const Duration(seconds: 20));
    }
    if (response.statusCode != HttpStatus.ok) {
      throw HttpException('GitHub HTTP ${response.statusCode}');
    }
    await verifyTo(response.timeout(const Duration(seconds: 25)), file);
  }

  Future<void> verifyTo(Stream<List<int>> bytes, File file) async {
    var received = 0;
    final writer = await file.open(mode: FileMode.write);
    try {
      final digest = await sha256
          .bind(
            bytes.asyncMap((chunk) async {
              received += chunk.length;
              if (received > size) {
                throw const FormatException('Update exceeds declared size');
              }
              await writer.writeFrom(chunk);
              return chunk;
            }),
          )
          .single;
      if (received != size || digest.toString() != sha) {
        throw const FormatException('Update integrity check failed');
      }
    } finally {
      await writer.close();
    }
  }
}

class UpdateTile extends StatelessWidget {
  const UpdateTile({super.key, required this.updates, required this.text});
  final ReleaseUpdates updates;
  final LocalText text;

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: updates,
    builder: (context, _) {
      final current = updates.installed, release = updates.latest;
      final newer =
          current != null && release != null && release.version > current;
      final status = switch ((current, release)) {
        (null, _) => 'GitHub Releases',
        (final v, null) => '$v',
        (final v?, final published?) when published.version == v =>
          '$v · ${text('Up to date', '已是最新版本')}',
        (final v?, final published?) when published.version > v =>
          '$v → ${published.version}',
        (final v, final published?) =>
          '$v · GitHub Releases v${published.version}',
      };
      return Column(
        children: [
          ListTile(
            leading: const Icon(Icons.system_update_alt_rounded),
            title: Text(text('Check for updates', '检查更新')),
            subtitle: Text(updates.error ?? status),
            trailing: updates.checking || updates.downloading
                ? const SizedBox.square(
                    dimension: 20,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.refresh_rounded),
            onTap: updates.checking || updates.downloading
                ? null
                : updates.check,
          ),
          if (newer)
            ListTile(
              leading: const Icon(Icons.download_outlined),
              title: Text(text('Download update', '下载更新')),
              subtitle: Text('GitHub Releases · ${release.version}'),
              onTap: updates.downloading || updates.checking
                  ? null
                  : updates.download,
            ),
        ],
      );
    },
  );
}
