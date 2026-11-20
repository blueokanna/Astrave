import 'dart:ffi';

import 'package:jni/jni.dart' show toJValues, using;
import 'package:jni_flutter/jni_flutter.dart';

@Native<Bool Function(Pointer<Void>, Pointer<Void>)>(
  symbol: 'astrave_init_android_tls',
  assetId: 'package:astrave_desktop/src/rust/frb_generated.io.dart',
)
external bool _initialize(Pointer<Void> vm, Pointer<Void> context);

void initializeAndroidTls() {
  final context = androidApplicationContext;
  try {
    final vm = DynamicLibrary.open('libdartjni.so')
        .lookupFunction<Pointer<Void> Function(), Pointer<Void> Function()>(
          'GetJavaVM',
        );
    final initialized = using((arena) {
      final argument = toJValues([context], allocator: arena);
      return _initialize(vm(), argument.ref.l);
    });
    if (!initialized) {
      throw StateError('Could not initialize the Android system trust manager');
    }
  } finally {
    context.release();
  }
}
