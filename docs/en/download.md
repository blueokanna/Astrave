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
