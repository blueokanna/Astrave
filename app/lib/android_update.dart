import 'package:jni/jni.dart';
import 'package:jni_flutter/jni_flutter.dart';

/// Hand a read-only content URI to the system installer. Android verifies the
/// package's signing identity and asks the device owner to approve the update.
void openAndroidUpdate(String path) => using((arena) {
  final context = androidApplicationContext..releasedBy(arena);
  final fileClass = JClass.forName('java/io/File')..releasedBy(arena);
  final file = fileClass.constructorId('(Ljava/lang/String;)V')(fileClass, [
    path.toJString()..releasedBy(arena),
  ])..releasedBy(arena);
  final provider = JClass.forName('androidx/core/content/FileProvider')
    ..releasedBy(arena);
  final uri =
      provider.staticMethodId(
          'getUriForFile',
          '(Landroid/content/Context;Ljava/lang/String;Ljava/io/File;)Landroid/net/Uri;',
        )(provider, JObject.type, [
          context,
          'dev.astrave.astrave.updates'.toJString()..releasedBy(arena),
          file,
        ])
        ..releasedBy(arena);
  final intentClass = JClass.forName('android/content/Intent')
    ..releasedBy(arena);
  final intent = intentClass.constructorId('(Ljava/lang/String;)V')(
    intentClass,
    ['android.intent.action.VIEW'.toJString()..releasedBy(arena)],
  )..releasedBy(arena);
  intentClass
      .instanceMethodId(
        'setDataAndType',
        '(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;',
      )(intent, JObject.type, [
        uri,
        'application/vnd.android.package-archive'.toJString()
          ..releasedBy(arena),
      ])
      .release();
  // FLAG_ACTIVITY_NEW_TASK | FLAG_GRANT_READ_URI_PERMISSION
  intentClass
      .instanceMethodId('addFlags', '(I)Landroid/content/Intent;')(
        intent,
        JObject.type,
        [0x10000001],
      )
      .release();
  final contextClass = context.jClass..releasedBy(arena);
  contextClass.instanceMethodId('startActivity', '(Landroid/content/Intent;)V')(
    context,
    jvoid.type,
    [intent],
  );
});
