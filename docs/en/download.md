---
sidebar_position: 2
sidebar_label: Download
---



# Build and download



The Releases page lists the CLI archives, desktop packages and signed Android APKs for each published tag. Choose `arm64-v8a` for a 64-bit ARM phone, `armeabi-v7a` for a 32-bit ARM phone or `x86_64` for an Intel/AMD Android device or emulator. [GitHub Releases](https://github.com/blueokanna/Astrave/releases)

Build the server and CLI from the repository root. The same Cargo package also builds the site generator and Flutter native library. For app setup, see the [Android and desktop guide](./desktop.md).

```shell
cargo build --locked --release --bin astrave --bin astrave-server
```

## Publish a new version

Update the version in `rust/Cargo.toml` and `app/pubspec.yaml` in the same push. Use the same version in both files and increment Flutter's `+` build number for Android upgrades. On a push to `main`, the Release workflow builds and publishes the version if it has no tag yet. You can also select `main` under **Actions → Release → Run workflow** to publish the current version, or push its `vX.Y.Z` tag.

The workflow signs three ABI-specific APKs using the repository secrets `ANDROID_KEYSTORE_BASE64`, `ANDROID_STORE_PASSWORD`, `ANDROID_KEY_ALIAS` and `ANDROID_KEY_PASSWORD`, and checks the release certificate fingerprint. Windows ZIPs, macOS DMGs and Linux DEBs cover both amd64 and arm64. The workflow pins the tag to the commit it built and compares each GitHub Release asset's SHA-256 with the staged package. Regular CI runs test development builds.
