# 原生 Cloudflare Tunnel

Astrave 可在进程内运行 Cloudflare 连接器，使用控制台提供的 Tunnel Token
直接向 Cloudflare 注册命名隧道。此模式不需要安装或运行 cloudflared，也不需要
astrave-server、Astrave 认证口令或为公网域名准备源站证书。

## 桌面端

1. 在「配置」页点击 **Cloudflare Tunnel**。
2. 粘贴 Cloudflare 隧道“安装连接器”页面中的 **Tunnel Token**。
   Tunnel ID 可留空；填写时会核对它是否与 Token 对应。
3. 选择自动、QUIC 或 HTTP/2，点击“使用此配置”。
4. 检查编辑器中的配置，点击“校验并保存”，再到「概览」点击“连接”。
5. 在 Cloudflare 控制台为该隧道添加 Published application 路由，例如
   `app.example.com` → `http://127.0.0.1:8080`。

这里的 `127.0.0.1` 指运行 Astrave 的设备。概览显示收到的公共域名，活动日志记录
连接注册和远程配置应用情况。Cloudflare 会为公共 HTTPS 入口提供证书；如果选择
HTTPS 源站，仍需源站本身提供可校验的证书，或配置自有 CA。

Tunnel Token、DNS API Token 和 Access Service Token 是不同凭据。此处使用
Tunnel Token；证书管理页的 Cloudflare DNS API 密钥不能用于注册隧道。

## 命令行

复制 `conf/cloudflare.toml`，给 Astrave 进程设置 `CLOUDFLARE_TUNNEL_TOKEN`，或将配置
改为 `tokenFile` 指向只含 Token 的 UTF-8 文件。相对文件路径以配置文件所在目录为准。
不要同时填写 `token` 和 `tokenFile`。

```powershell
.\astrave.exe verify -c conf/cloudflare.toml
.\astrave.exe -c conf/cloudflare.toml
```

配置使用 `[cloudflare]`，不与 Astrave 网关的 `[[tunnels]]` 混用。
Token 文件不会写入日志；Token 解析错误也不会包含解码后的凭据。
更新本地设置后可重载配置，会重新注册连接；切换到 Astrave 网关模式前先断开。
远程路由配置会从 Cloudflare 下发并自动更新。

## 当前兼容范围

| 能力 | 实现情况 |
| --- | --- |
| 控制台管理的命名隧道及 Token 认证 | 原生注册；可校验 Tunnel ID |
| 边缘传输 | QUIC、HTTP/2、自动回退，最多四条连接，断线重连 |
| 远程 ingress | 按顺序匹配域名、通配域名和路径正则；末尾 catch-all；整份验证后应用 |
| HTTP/HTTPS 源站 | 请求和响应流式转发、状态码、重复响应头、SSE、Host 改写 |
| WebSocket 源站 | HTTP Upgrade 和双向原始帧转发 |
| 公共 TCP 服务 | `tcp://`、`ssh://`、`rdp://`、`smb://`，通过客户端 WebSocket 封装访问 |
| 源站 TLS | 系统信任、自有 CA、originServerName、matchSNItoHost、显式 noTLSVerify |
| 源站连接 | 使用系统 DNS。IPv4/IPv6 拨号错开 250 ms，每次请求最多同时尝试两个地址，失败后立即补位。`noHappyEyeballs` 启用顺序拨号。支持连接及 TLS 超时、TCP keepalive |
| HTTP/2/gRPC 源站、HTTP trailers | 尚未实现 |
| WARP 私网 TCP/UDP/ICMP、bastion、SOCKS、Unix socket | 尚未实现 |
| 源站 Access JWT 校验、远程管理日志协议 | 尚未实现 |
| 本地管理 credentials.json、quick tunnel 命令 | 尚未提供用户入口 |
| 源站连接池、keepAliveTimeout / keepAliveConnections 调优 | 尚未实现；每个请求单独建立源站连接 |

不支持的源站设置或私网配置会返回配置错误，保留上一份有效配置。
这不是 cloudflared 的全部功能等价实现。测试不会把本地模拟或 quick tunnel
验证当作用户账号命名隧道的公网验收；实际 Token、路由、Access 策略和网络环境需要
另外联调。

## 验证

离线测试：

```sh
cargo test -p astrave --features app --lib cloudflare
```

以下显式测试会创建临时公共 quick tunnel，只转发固定测试响应，分别检查 HTTP/2
和 QUIC 公网数据面；默认测试不会运行它：

```sh
cargo test -p astrave --features app --lib live_native_cloudflare_http2_and_quic -- --ignored --nocapture
```

2026-11-25 在 Windows 上运行该测试时，HTTP/2 和 QUIC 均完成注册，公共 HTTPS 请求均收到预期的 `astrave-native-edge-proof` 响应。账号命名隧道仍需使用对应的 Tunnel Token、公共域名和控制台路由另行验证。
