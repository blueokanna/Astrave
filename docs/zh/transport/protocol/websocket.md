---
sidebar_position: 2
sidebar_label: WebSocket
title: WebSocket
---

# WebSocket

与 TCP 共用服务端 `listen`。适用于需穿透 HTTP 代理、或仅开放 WebSocket 的网络环境。TLS 配置见 [TLS](../tls.md)。

## 示例

服务端：

```toml
# astrave-server.toml
listen = "0.0.0.0:9527"

[transport]
tcpMux = true
wsPath = "/ws"
```

客户端：

```toml
# astrave.toml
server = "YOUR_SERVER_IP:9527"

[transport]
protocol = "websocket"
tcpMux = true
heartbeatInterval = -1
wsPath = "/ws"
```

## 客户端参数

| 参数                          | 必填 | 默认值      | 说明                                                                                                  |
|-------------------------------|------|-------------|-------------------------------------------------------------------------------------------------------|
| `transport.protocol`          | 是   | `tcp`       | `websocket`（或 `ws`）直连，`wss` 接入 HTTPS 反向代理 |
| `transport.wsTrustedCaFile`   | 否   | 系统信任库 | WSS 入口使用私有 CA 时填写 PEM 文件，相对路径以客户端配置所在目录为准 |
| `transport.wsHeaderEnv`       | 否   | 空 | 将 WSS 入口请求头映射到进程环境变量名。[带鉴权的入口配置](../../cloud.md) |
| `transport.wsPath`            | 否   | `/~!astrave` | WebSocket HTTP 路径；须与服务端一致                                                                   |
| `transport.tcpMux`            | 否   | `true`      | TCP 多路复用；需与服务端一致                                                                          |
| `transport.heartbeatInterval` | 否   | `-1`        | 应用心跳间隔（秒）；`-1` 关闭。关闭 `tcpMux` 时自动变为 `30`                                          |
| `transport.heartbeatTimeout`  | 否   | `-1`        | 心跳超时（秒）；未收到 Pong 则断开。关闭 `tcpMux` 时自动变为 `90`；详见 [TCP 多路复用](../tcp-mux.md) |

## 服务端参数

| 参数               | 必填 | 默认值         | 说明                                |
|--------------------|------|----------------|-------------------------------------|
| `listen`           | 是   | `0.0.0.0:9527` | 与 TCP 共用监听地址                 |
| `transport.wsPath` | 否   | `/~!astrave`    | WebSocket HTTP 路径；须与客户端一致 |
| `transport.tcpMux` | 否   | `true`         | TCP 多路复用；需与客户端一致        |

## 通过 Cloudflare Tunnel 连接网关

将 `cloudflared` 与 `astrave-server` 部署在同一台主机。客户端用 `server` 的主机名校验 Cloudflare 入口证书，WebSocket 升级后再用 `transport.tls.serverName` 校验 Astrave 源站。客户端证书只参与内层 mTLS。该入口承载 Astrave 客户端到网关的连接。映射服务的访问地址仍由网关配置决定。

以下为本地管理的命名 Tunnel 示例。将域名、Tunnel UUID 和证书路径替换为实际值。在区域中开启 WebSockets，把 `edge.example.com` 的 DNS 路由指向该 Tunnel。

```yaml
# cloudflared config.yml
tunnel: <TUNNEL-UUID>
credentials-file: /etc/cloudflared/<TUNNEL-UUID>.json
ingress:
  - hostname: edge.example.com
    path: ^/ws$
    service: http://127.0.0.1:9527
  - service: http_status:404
```

```toml
# astrave-server.toml
listen = "127.0.0.1:9527"
proxyAddr = "127.0.0.1"
auth.token = "{{env.ASTRAVE_TOKEN}}"
transport.wsPath = "/ws"
transport.tls.force = true
transport.tls.certFile = "origin.pem"
transport.tls.keyFile = "origin.key"
transport.tls.trustedCaFile = "client-ca.pem"
```

```toml
# astrave.toml，可接已有的 [[tunnels]] 配置
server = "edge.example.com:443"
auth.token = "{{env.ASTRAVE_TOKEN}}"
transport.protocol = "wss"
transport.wsPath = "/ws"
transport.heartbeatInterval = 30
transport.heartbeatTimeout = 90
transport.tls.enable = true
transport.tls.serverName = "origin.internal.example"
transport.tls.trustedCaFile = "origin-ca.pem"
transport.tls.certFile = "client.pem"
transport.tls.keyFile = "client.key"
```

`origin.pem` 的 DNS SAN 需包含 `origin.internal.example`。在两个 Astrave 进程中设置相同的 `ASTRAVE_TOKEN`。验证入口配置后启动 Tunnel，再启动客户端。边缘连接中断时，客户端会重新建立会话，当时正在传输的业务连接需要重新连接。

```sh
cloudflared tunnel ingress validate
cloudflared tunnel ingress rule https://edge.example.com/ws
cloudflared tunnel run <TUNNEL-UUID>
```

若域名还受 Cloudflare Access 保护，在 Access 应用中为该服务令牌配置 **Service Auth** 策略，在 Astrave 客户端配置中加入下表，并给客户端进程设置这两个环境变量。客户端会在每次 WSS Upgrade 时发送服务令牌。无人值守客户端使用此方式认证。[Cloudflare 服务令牌](https://developers.cloudflare.com/cloudflare-one/access-controls/service-credentials/service-tokens/)

```toml
[transport.cloudflareAccess]
clientId = "{{env.CF_ACCESS_CLIENT_ID}}"
clientSecret = "{{env.CF_ACCESS_CLIENT_SECRET}}"
```

[Cloudflare Tunnel 入口规则](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/local-management/configuration-file/) · [Cloudflare WebSockets](https://developers.cloudflare.com/network/websockets/)
