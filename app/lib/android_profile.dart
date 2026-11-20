import 'dart:convert';
import 'dart:io';
import 'dart:isolate';
import 'dart:typed_data';

import 'package:android_file_picker/android_file_picker.dart';
import 'package:jni/jni.dart';
import 'package:jni_flutter/jni_flutter.dart';

const _maxProfileBytes = 2 * 1024 * 1024;

Future<String> readProfileText(File file) async {
  final input = await file.open();
  try {
    final size = await input.length();
    if (size > _maxProfileBytes) {
      throw const FormatException('Profile exceeds 2 MiB');
    }
    final bytes = await input.read(size + 1);
    if (bytes.length != size) {
      throw const FormatException('Profile changed while reading');
    }
    return utf8.decode(bytes);
  } finally {
    await input.close();
  }
}

Future<String?> pickAndroidProfile() async {
  final picked = await FilePickerAndroid().pickFile(
    androidOptions: const FilePickerAndroidOptions(
      safOptions: AndroidSAFOptions(persistGrant: false, copyToCache: false),
    ),
  );
  if (picked == null) return null;
  final uri = picked.uri;
  if (uri.scheme == 'file') {
    final copy = File.fromUri(uri);
    try {
      return await readProfileText(copy);
    } finally {
      await copy.delete();
    }
  }
  return Isolate.run(() => _readAndroidDocument(uri.toString()));
}

// A document provider may stream a file of unknown length. Limit the bytes
// consumed and perform all provider I/O away from the Flutter UI isolate.
String _readAndroidDocument(String uri) => using((arena) {
  final uriClass = JClass.forName('android/net/Uri')..releasedBy(arena);
  final parse = uriClass.staticMethodId(
    'parse',
    '(Ljava/lang/String;)Landroid/net/Uri;',
  );
  final uriText = uri.toJString()..releasedBy(arena);
  final document = parse(uriClass, JObject.type, [uriText])..releasedBy(arena);
  final context = androidApplicationContext..releasedBy(arena);
  final contextClass = context.jClass..releasedBy(arena);
  final getResolver = contextClass.instanceMethodId(
    'getContentResolver',
    '()Landroid/content/ContentResolver;',
  );
  final resolver = getResolver(context, JObject.type, [])..releasedBy(arena);
  final resolverClass = resolver.jClass..releasedBy(arena);
  final open = resolverClass.instanceMethodId(
    'openInputStream',
    '(Landroid/net/Uri;)Ljava/io/InputStream;',
  );
  final input = open(resolver, JObject.type, [document])..releasedBy(arena);
  final streamClass = JClass.forName('java/io/InputStream')..releasedBy(arena);
  final close = streamClass.instanceMethodId('close', '()V');
  try {
    final read = streamClass.instanceMethodId('read', '([B)I');
    final buffer = JByteArray(16 * 1024)..releasedBy(arena);
    final bytes = BytesBuilder(copy: false);
    while (true) {
      final count = read(input, jint.type, [buffer]);
      if (count == -1) return utf8.decode(bytes.takeBytes());
      if (count <= 0) throw const FormatException('Could not read profile');
      if (bytes.length + count > _maxProfileBytes) {
        throw const FormatException('Profile exceeds 2 MiB');
      }
      bytes.add(buffer.getRange(0, count).buffer.asUint8List());
    }
  } finally {
    close(input, jvoid.type, []);
  }
});
