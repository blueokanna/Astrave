---
sidebar_position: 4
sidebar_label: KCP
title: KCP
---

# KCP

基于 UDP 的可靠传输，弱网场景更稳。TLS 配置见 [TLS](../tls.md)。

## 示例

服务端：

```toml
# astrave-server.toml
listen = "0.0.0.0:9527"
kcpPort = 9529

[transport]
tcpMux = true
```

客户端：

```toml
# astrave.toml
server = "YOUR_SERVER_IP:9529"

[transport]
protocol = "kcp"
tcpMux = true
heartbeatInterval = -1
```

`server` 需指向服务端 `kcpPort`，且不可与 `quicPort` 相同。

对于 IPv6 网关，`server` 可填域名，也可填带方括号的地址，例如 `server = "[2001:db8::1]:9529"`。如果域名解析出多个地址，控制连接握手超时后，客户端会在重连时尝试下一个地址。网关明确拒绝登录时，客户端会停止运行。

## 客户端参数

| 参数                            | 必填 | 默认值    | 说明                                       |
|-------------------------------|----|--------|------------------------------------------|
| `transport.protocol`          | 是  | `tcp`  | 固定为 `kcp`                                |
| `server`                      | 是  |        | 填服务端地址，端口为 `kcpPort`                    |
| `transport.tcpMux`            | 否  | `true` | TCP 多路复用；需与服务端一致                         |
| `transport.heartbeatInterval` | 否  | `-1`   | 应用心跳间隔（秒）；`-1` 关闭。关闭 `tcpMux` 时自动变为 `30` |
| `transport.heartbeatTimeout` | 否  | `-1` | 心跳超时（秒）；未收到 Pong 则断开。关闭 `tcpMux` 时自动变为 `90`；详见 [TCP 多路复用](../tcp-mux.md) |

## 服务端参数

| 参数                 | 必填 | 默认值    | 说明                                      |
|--------------------|----|--------|-----------------------------------------|
| `kcpPort`          | 是  | `0`    | KCP 监听端口（UDP）；`0` 表示关闭；不可与 `quicPort` 相同 |
| `transport.tcpMux` | 否  | `true` | TCP 多路复用；需与客户端一致                        |
