---
sidebar_position: 5
sidebar_label: Android and Desktop
title: Android and Desktop Clients
---

# Android and desktop clients

Open a TOML file under Profile, validate and save it, then connect from Overview. The connection card shows the client state and the transport settings from the profile. If you save changes during a session, select the pending-reload message to open Profile and choose **Reload saved profile**.

Tunnel rows show the path incoming traffic follows from the public endpoint to the local service. Copy the assigned endpoint from the row menu once the server confirms it. A dynamically assigned port reads **Automatic** until the endpoint arrives. Activity shows the most recent 500 log entries. Open **Settings → Certificates** to configure a DNS provider and request a certificate.

Settings offers Simplified and Traditional Chinese, English, German, French, Spanish, Arabic, Portuguese, Dutch, Italian, Romanian, Japanese, Korean, Hindi and Russian. System default follows the first supported preferred language. Arabic uses a right-to-left layout. Choose AMOLED for a black background, or AMOLED Monet for black surfaces with system accent colors. Where system colors are unavailable, the app generates colors from the selected seed. Fourteen seeds are available. The floating navigation capsule supports solid, Gaussian blur, Aero Glass and Liquid Glass styles. High contrast uses a solid surface. Choices persist across restarts.

Activity labels INFO, WARN and ERROR and combines severity filtering with text search. **Settings → About Astrave** shows version and process usage. **Check for updates** reads the latest stable GitHub Release on startup and every 15 minutes while in the foreground. Tap it to refresh immediately. When a newer build is available, **Download update** checks the selected architecture, byte count and SHA-256 before opening the package. Approve the Android installer, open the macOS disk image or use the Linux package installer. On Windows extract the portable ZIP to a new folder, close the old copy and launch the executable in the new folder. If no matching asset is published, the app opens the release page.

On an Android 13 x86_64 emulator, the updater downloaded the development APK, shared it with the system installer through FileProvider, and upgraded a test build from version code 4001 to 4002 after confirmation. Both packages used the same development key. On Windows x64, the updater verified the published v1.0.0 ZIP and successfully handed it to the system file handler.

### Windows client, version 1.0.1

These screenshots show the Simplified Chinese interface during one loopback TCP session. The gateway assigned port 54751 to `local-api`. After the gateway stopped, Activity recorded the disconnect and retry. Settings shows the installed development version alongside the latest published GitHub release.

| Overview | Profile |
| --- | --- |
| ![Connected TCP tunnel and assigned endpoint](/img/windows-1.0.1-overview.png) | ![TOML profile for the connected session](/img/windows-1.0.1-profile.png) |
| **Activity** | **Settings** |
| ![Disconnection warning and reconnect events](/img/windows-1.0.1-activity.png) | ![Preferences, certificates, updates and About](/img/windows-1.0.1-settings.png) |

### DNS-01 and delegated zones

**Settings → Certificates** can create temporary TXT records with credentials for Cloudflare, Alibaba Cloud DNS and Tencent DNSPod. Enter the certificate names separated by commas. For a hostname you control on Oray, including one used with PeanutHull or 贝锐蒲公英, first publish a CNAME from `_acme-challenge.app.example.org` to an owner you control, such as `_proof.validation.example.net`. Select the provider for `validation.example.net` and enter `app.example.org=_proof.validation.example.net`. For a wildcard use `*.example.org=_proof.validation.example.net` and place the CNAME at `_acme-challenge.example.org`. The client checks propagation through the original name and cleans up TXT records it created. This requires permission to edit the challenge name’s authoritative DNS. A provider-issued hostname with no DNS editing rights requires a custom domain under your control.

### Earlier release measurements

<img src="../img/android-settings.png" alt="Settings on an Android 13 emulator with the floating navigation capsule" width="320">

![Settings on Windows in dark mode](/img/desktop-settings.png)

![Astrave Desktop](/img/desktop.png)

The four destinations share a Material 3 Expressive spring. One Flutter ticker uses the display's vsync timestamps to position and fade both pages during paint. The system selects the active refresh mode. Reversing direction preserves the current positions and the editor state. Reduced motion switches pages immediately. Widget tests exercise simulated 60, 90 and 120 Hz frame intervals and a change from 60 to 120 Hz during a transition.

On Windows, a Profile build visited four destinations eight times at 140 ms intervals after warm-up. The previous sample contained 338 frames with p95 build and raster times of 5.33 and 4.63 ms. The current sample contained 289 frames with p95 times of 3.26 and 3.50 ms. Neither stage exceeded 16.67 ms in the current sample. Flutter measures the two stages separately. Raw samples are in `docs/benchmarks/windows-navigation-frames.csv` and `docs/benchmarks/flat-windows-navigation.csv`.

On Android, use **Profile → Import profile** to load a document into the editor. Saving writes the profile to app-private storage. Relative certificate paths resolve from that local profile. Use an address reachable from the phone for the server, and the phone’s loopback address for services running on the phone. The tunnel shares the app’s process lifetime. Android may suspend it in the background.

The Android document picker grants read access to the selected document. Astrave reads content URIs in 16 KiB chunks on a background Dart isolate and stops when the profile exceeds 2 MiB, including when the provider supplies no size. An import takes linear time in the bytes read. The editor holds the decoded TOML in memory so you can review it before saving. While the app is in the background it pauses status polling and refreshes on return.

<img src="../img/android.png" alt="Connected TCP tunnel over TLS on an Android 13 emulator" width="320">

On the software-rendered x86_64 emulator, SurfaceFlinger reported 14 positive intervals between successive presentations of the Flutter surface across four tab changes. Intervals ranged from 47.3 to 374.3 ms with a median of 107.2 ms, including pauses between updates. These samples describe emulator presentation timing. The Windows sample above measures different stages of the Flutter pipeline. The Android samples are in `docs/benchmarks/android-emulator-navigation.csv`.

The Android 13 (API 33) x86_64 emulator test imported TOML through the system picker, checked the saved file byte for byte, and sent eight 32 KiB payloads through the published TCP port. Each response matched its request. The Windows host ran the TLS server and echo service on loopback, with `adb reverse` providing access from Android. The test CA was supplied separately in the emulator’s app-private directory. After removing `trustedCaFile`, the test server received an `unknown_ca` TLS alert from Android. A separate check sent an echo after pressing Home, returned to the app, disconnected, and verified the saved profile after relaunch.

All three split APKs from the single-package build passed APK v2 signature verification, ZIP CRC checks and `zipalign -c -P 16 4`. Each included the Flutter engine and a Rust library for its stated ABI with the `astrave_init_android_tls` symbol. The Rust library’s ELF load segments had 16 KiB minimum alignment on arm64-v8a and x86_64, and 4 KiB on armeabi-v7a. The local packages used the development key. One run on the software-only emulator hit an Android input timeout while switching languages. Physical phone performance and long-running sessions during Doze have not been measured.
