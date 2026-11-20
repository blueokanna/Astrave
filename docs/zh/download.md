---
sidebar_position: 2
sidebar_label: 下载
---



# 构建与下载



每个已发布的 tag 都可在 Releases 页面找到对应的命令行归档、桌面安装包和已签名的 Android APK。64 位 ARM 手机选择 `arm64-v8a`，32 位 ARM 手机选择 `armeabi-v7a`，Intel 或 AMD 的 Android 设备及模拟器选择 `x86_64`。[GitHub Releases](https://github.com/blueokanna/Astrave/releases)

在项目根目录执行以下命令即可构建服务端和命令行客户端。站点生成器与 Flutter 原生库也由同一个 Cargo 包构建。应用配置见 [Android 与桌面客户端](./desktop.md)。

```shell
cargo build --locked --release --bin astrave --bin astrave-server
```
