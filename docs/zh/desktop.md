---
sidebar_position: 5
sidebar_label: Android 与桌面端
title: Android 与桌面客户端
---

# Android 与桌面客户端

在「配置」中打开 TOML 文件，校验并保存，然后在「概览」中连接。连接卡显示客户端状态和配置文件中的传输参数。连接期间手动保存配置后，概览会提示等待重载。点击提示进入配置页，选择「重载已保存的配置」。

隧道列表按入站流量的方向排列，从映射端点指向本地服务。服务器确认分配后，行内显示可复制的实际端点。自动分配的端口在收到结果前显示为「自动」。「活动日志」显示最近 500 条记录。在「设置 → 证书」中添加 DNS 服务商后即可申请证书。

在设置中可选择语言、明暗主题、系统动态色或预设配色。Android 悬浮导航胶囊支持纯色、高斯模糊、Aero Glass 着色玻璃和 Liquid Glass 风格。玻璃背景取样自胶囊下方的页面内容。Liquid Glass 在 Impeller 上增加边缘折射，其他渲染器使用模糊和半透明渐变；高对比度模式使用纯色。偏好会在重启后恢复。

![Astrave 桌面客户端的已连接隧道](/img/desktop-zh.png)

四个页面共用 Material 3 Expressive 弹簧动效。Flutter 使用同一个 ticker，根据屏幕的 vsync 时间戳在绘制阶段更新两个页面的位置和透明度。当前刷新模式由系统选择。中途反向操作会保留页面位置和编辑状态，开启系统减弱动态效果时立即换页。组件测试覆盖了模拟的 60、90、120 Hz 帧间隔，以及切换途中从 60 升至 120 Hz 的情况。

在 Windows 上，Profile 构建预热后每 140 ms 切页，循环访问四个页面八次。前一版记录 338 帧，构建与栅格化耗时的 p95 分别为 5.33 和 4.63 ms。本版记录 289 帧，两项 p95 分别为 3.26 和 3.50 ms，两个阶段均未出现超过 16.67 ms 的样本。Flutter 分别统计这两个流水线阶段。原始记录见 `docs/benchmarks/windows-navigation-frames.csv` 和 `docs/benchmarks/flat-windows-navigation.csv`。

在 Android 上，通过「配置 → 导入配置」将文档读入编辑器，保存后配置存放在应用私有目录。证书的相对路径以这份本机配置为基准。服务器地址应填写手机能够访问的地址；手机自身运行的服务可以使用手机的回环地址。隧道随应用进程运行，切至后台时 Android 可能将其挂起。

系统选择器授予应用读取所选文档的权限。Astrave 在后台 Dart isolate 中每次读取 16 KiB，累计超过 2 MiB 就停止，即使文档提供方没有报告文件大小也会执行此限制。导入耗时与实际读取的字节数成正比。编辑器在内存中保留解码后的 TOML，供保存前检查。应用切到后台时暂停状态轮询，回到前台时立即刷新。

<img src="../img/android-zh.png" alt="Android 13 模拟器中已通过 TLS 连接的 TCP 隧道" width="320">

在纯软件渲染的 x86_64 模拟器上，切换四次标签页时，SurfaceFlinger 共记录到 14 个大于零的 Flutter 画面呈现间隔。间隔包含画面更新之间的停顿，范围为 47.3–374.3 ms，中位数为 107.2 ms。这组数值衡量模拟器的画面呈现间隔，与前文 Windows 上分别测量的构建、栅格化耗时口径不同。原始记录见 `docs/benchmarks/android-emulator-navigation.csv`。

在 Android 13（API 33）x86_64 模拟器上，测试通过系统选择器导入 TOML，逐字节核对保存后的文件，并向已发布的 TCP 端口发送了 8 次 32 KiB 载荷，每次响应均与请求一致。TLS 服务器和回显服务运行在 Windows 主机的回环地址上，手机通过 `adb reverse` 访问。测试 CA 单独放入模拟器的应用私有目录。移除 `trustedCaFile` 后，测试服务器收到了 Android 发出的 `unknown_ca` TLS 告警。另一次检查在按 Home 后完成回传，返回应用后断开连接，并确认重启后配置仍在。

合包后按 ABI 构建的三个 APK 均通过 APK v2 签名验证、ZIP CRC 检查和 `zipalign -c -P 16 4` 校验。每个包都包含对应架构的 Flutter 引擎和 Rust 库，且 Rust 库导出了 `astrave_init_android_tls`。Rust 库 ELF 加载段的最小对齐在 arm64-v8a 和 x86_64 上为 16 KiB，在 armeabi-v7a 上为 4 KiB。本地包使用开发签名。纯软件模拟器有一次在切换语言时触发了 Android 输入超时。实体手机的性能和省电休眠中的长时间连接仍待实测。
