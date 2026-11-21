# Rust looks up the verifier and reads its result fields by their JNI names.
-keep class org.rustls.platformverifier.** { *; }
# The updater calls this static method by JNI name to grant access to one APK.
-keepclassmembers class androidx.core.content.FileProvider {
    public static android.net.Uri getUriForFile(android.content.Context, java.lang.String, java.io.File);
}
