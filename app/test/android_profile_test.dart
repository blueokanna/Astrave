import 'dart:io';

import 'package:astrave_desktop/android_profile.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('imports UTF-8 and enforces the profile byte limit', () async {
    final directory = await Directory.systemTemp.createTemp('astrave-import-');
    addTearDown(() => directory.delete(recursive: true));
    final file = File('${directory.path}/profile.toml');
    const source = '# 手机配置\nserver = "localhost:9527"\n';
    await file.writeAsString(source);
    expect(await readProfileText(file), source);
    final handle = await file.open(mode: FileMode.append);
    await handle.truncate(2 * 1024 * 1024 + 1);
    await handle.close();
    await expectLater(readProfileText(file), throwsFormatException);
  });
}
