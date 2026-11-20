import java.util.Properties

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

val signingFile = rootProject.file("key.properties")
val releaseKey = Properties().apply {
    if (signingFile.isFile) signingFile.inputStream().use { load(it) }
}
val ciKeystore = providers.environmentVariable("ASTRAVE_KEYSTORE").orNull

android {
    namespace = "dev.astrave.astrave"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        applicationId = "dev.astrave.astrave"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter. (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    signingConfigs {
        if (signingFile.isFile || ciKeystore != null) create("release") {
            storeFile = rootProject.file(ciKeystore ?: releaseKey.getProperty("storeFile"))
            storePassword = System.getenv("ASTRAVE_STORE_PASSWORD")?.ifEmpty { null } ?: releaseKey.getProperty("storePassword")
            keyAlias = System.getenv("ASTRAVE_KEY_ALIAS")?.ifEmpty { null } ?: releaseKey.getProperty("keyAlias")
            keyPassword = System.getenv("ASTRAVE_KEY_PASSWORD")?.ifEmpty { null } ?: releaseKey.getProperty("keyPassword")
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}

// The AAR supplies the system trust manager called by the locked Rust crate.
val cargoLock = providers.fileContents(layout.projectDirectory.file("../../../Cargo.lock")).asText.get()
val verifierVersion = Regex("""name = "rustls-platform-verifier-android"\s+version = "([^"]+)"""")
    .find(cargoLock)?.groupValues?.get(1) ?: error("Missing Android verifier in Cargo.lock")
dependencies {
    implementation("org.rustls:rustls-platform-verifier:$verifierVersion")
}
