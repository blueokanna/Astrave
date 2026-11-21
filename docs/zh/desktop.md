---
sidebar_position: 5
sidebar_label: Android 与桌面端
title: Android 与桌面客户端
---

# Android 与桌面客户端

在「配置」中打开 TOML 文件，校验并保存，然后在「概览」中连接。连接卡显示客户端状态和配置文件中的传输参数。连接期间手动保存配置后，概览会提示等待重载。点击提示进入配置页，选择「重载已保存的配置」。

隧道列表按入站流量的方向排列，从映射端点指向本地服务。服务器确认分配后，行内显示可复制的实际端点。自动分配的端口在收到结果前显示为「自动」。「活动日志」显示最近 500 条记录。在「设置 → 证书」中添加 DNS 服务商后即可申请证书。

设置支持简体中文、繁体中文、英语、德语、法语、西班牙语、阿拉伯语、葡萄牙语、荷兰语、意大利语、罗马尼亚语、日语、韩语、印地语和俄语。「跟随系统」按系统偏好顺序匹配，阿拉伯语从右向左排版。AMOLED 使用纯黑背景，AMOLED Monet 在纯黑表面上使用系统动态色。系统未提供动态色时，可用所选种子色生成配色，共有 14 种预设。悬浮导航胶囊支持纯色、高斯模糊、Aero Glass 和 Liquid Glass，高对比度模式使用纯色。偏好在重启后恢复。

活动日志用文字和图标标明 INFO、WARN 和 ERROR，支持同时按级别和关键词筛选。「设置 → 关于 Astrave」显示版本和进程资源占用。「检查更新」在启动时及前台运行期间每 15 分钟读取一次 GitHub 最新稳定版，也可以点击立即刷新。发现新版后，点击「下载更新」会按架构选包，核对字节数和 SHA-256，再交给系统打开。Android 需要在系统安装器中确认，macOS 打开磁盘映像，Linux 用包安装器。Windows 使用便携 ZIP，解压到新目录、关闭旧客户端后，从新目录启动。缺少对应架构的附件时会打开发布页。

在 Android 13 x86_64 模拟器上，更新器下载开发版 APK 后，通过 FileProvider 交给系统安装器。确认安装后，系统报告版本号从 4001 升至 4002。两个包使用同一开发签名。在 Windows x64 上，更新器校验了正式发布的 v1.0.0 ZIP，并成功交给系统文件处理程序。

### Windows 客户端 1.0.1

四张截图来自同一次本机 TCP 会话。网关为 `local-api` 注册了 54751 端口；关闭网关后，活动日志记录了断线和重试。设置页同时列出了本地开发版本和 GitHub 最新正式版本。

| 概览 | 配置 |
| --- | --- |
| ![已连接的 TCP 隧道和分配端点](/img/windows-1.0.1-overview.png) | ![当前会话的 TOML 配置](/img/windows-1.0.1-profile.png) |
| **活动日志** | **设置** |
| ![断线警告和重试记录](/img/windows-1.0.1-activity.png) | ![偏好设置、证书、检查更新和关于入口](/img/windows-1.0.1-settings.png) |

### DNS-01 和验证委派

在「设置 → 证书」中配置 Cloudflare、阿里云 DNS 或腾讯云 DNSPod，可自动写入临时 TXT 记录。多个申请域名以逗号分隔。自有域名用于花生壳或蒲公英时，可在权威 DNS 中先把 `_acme-challenge.app.example.org` 配置为 CNAME，指向由自己控制的 `_proof.validation.example.net`。选择管理 `validation.example.net` 的服务商，填入 `app.example.org=_proof.validation.example.net`。申请通配符时，填 `*.example.org=_proof.validation.example.net`，CNAME 设置在 `_acme-challenge.example.org`。客户端从原域名查询验证值，只清理本次创建的 TXT。使用此方式需要有权修改挑战名称的权威 DNS。服务商分配的域名若无此权限，需要绑定可自行管理的域名。

### 此前版本的测量记录

<img src="../img/android-settings-zh.png" alt="Android 13 模拟器中的中文设置和悬浮胶囊导航栏" width="320">

![Windows 深色主题下的中文设置](/img/desktop-settings-zh.png)

![Astrave 桌面客户端的已连接隧道](/img/desktop-zh.png)

四个页面共用 Material 3 Expressive 弹簧动效。Flutter 使用同一个 ticker，根据屏幕的 vsync 时间戳在绘制阶段更新两个页面的位置和透明度。当前刷新模式由系统选择。中途反向操作会保留页面位置和编辑状态，开启系统减弱动态效果时立即换页。组件测试覆盖了模拟的 60、90、120 Hz 帧间隔，以及切换途中从 60 升至 120 Hz 的情况。

在 Windows 上，Profile 构建预热后每 140 ms 切页，循环访问四个页面八次。前一版记录 338 帧，构建与栅格化耗时的 p95 分别为 5.33 和 4.63 ms。本版记录 289 帧，两项 p95 分别为 3.26 和 3.50 ms，两个阶段均未出现超过 16.67 ms 的样本。Flutter 分别统计这两个流水线阶段。原始记录见 `docs/benchmarks/windows-navigation-frames.csv` 和 `docs/benchmarks/flat-windows-navigation.csv`。

在 Android 上，通过「配置 → 导入配置」将文档读入编辑器，保存后配置存放在应用私有目录。证书的相对路径以这份本机配置为基准。服务器地址应填写手机能够访问的地址；手机自身运行的服务可以使用手机的回环地址。隧道随应用进程运行，切至后台时 Android 可能将其挂起。

系统选择器授予应用读取所选文档的权限。Astrave 在后台 Dart isolate 中每次读取 16 KiB，累计超过 2 MiB 就停止，即使文档提供方没有报告文件大小也会执行此限制。导入耗时与实际读取的字节数成正比。编辑器在内存中保留解码后的 TOML，供保存前检查。应用切到后台时暂停状态轮询，回到前台时立即刷新。

<img src="../img/android-zh.png" alt="Android 13 模拟器中已通过 TLS 连接的 TCP 隧道" width="320">

在纯软件渲染的 x86_64 模拟器上，切换四次标签页时，SurfaceFlinger 共记录到 14 个大于零的 Flutter 画面呈现间隔。间隔包含画面更新之间的停顿，范围为 47.3–374.3 ms，中位数为 107.2 ms。这组数值衡量模拟器的画面呈现间隔，与前文 Windows 上分别测量的构建、栅格化耗时口径不同。原始记录见 `docs/benchmarks/android-emulator-navigation.csv`。

在 Android 13（API 33）x86_64 模拟器上，测试通过系统选择器导入 TOML，逐字节核对保存后的文件，并向已发布的 TCP 端口发送了 8 次 32 KiB 载荷，每次响应均与请求一致。TLS 服务器和回显服务运行在 Windows 主机的回环地址上，手机通过 `adb reverse` 访问。测试 CA 单独放入模拟器的应用私有目录。移除 `trustedCaFile` 后，测试服务器收到了 Android 发出的 `unknown_ca` TLS 告警。另一次检查在按 Home 后完成回传，返回应用后断开连接，并确认重启后配置仍在。

合包后按 ABI 构建的三个 APK 均通过 APK v2 签名验证、ZIP CRC 检查和 `zipalign -c -P 16 4` 校验。每个包都包含对应架构的 Flutter 引擎和 Rust 库，且 Rust 库导出了 `astrave_init_android_tls`。Rust 库 ELF 加载段的最小对齐在 arm64-v8a 和 x86_64 上为 16 KiB，在 armeabi-v7a 上为 4 KiB。本地包使用开发签名。纯软件模拟器有一次在切换语言时触发了 Android 输入超时。实体手机的性能和省电休眠中的长时间连接仍待实测。
