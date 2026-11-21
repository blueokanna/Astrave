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

## 发布新版本

在同一次推送中更新 `rust/Cargo.toml` 的版本和 `app/pubspec.yaml` 的 `version`，两者的版本号须一致，Android 的 `+` 构建号须递增。推送到 `main` 后，Release 工作流会为尚无标签的版本构建并发布。也可以在 **Actions → Release → Run workflow** 中选择 `main` 手动发布当前版本，或推送对应的 `vX.Y.Z` 标签。

发布流程使用仓库的 `ANDROID_KEYSTORE_BASE64`、`ANDROID_STORE_PASSWORD`、`ANDROID_KEY_ALIAS` 和 `ANDROID_KEY_PASSWORD` 签名三个 ABI 的 APK，并核对发布证书指纹。Windows ZIP、macOS DMG、Linux DEB 各提供 amd64 和 arm64 两种架构。工作流会将标签绑定到实际构建的提交，并逐一比对 GitHub Release 附件的 SHA-256。普通 CI 运行用于测试开发构建。
