---
sidebar_position: 1
sidebar_label: TCP
title: TCP
---

# TCP

默认传输协议。客户端通过服务端 `listen` 建立控制通道。TLS 配置见 [TLS](../tls.md)。

## 示例

服务端：

```toml
# astrave-server.toml
listen = "0.0.0.0:9527"

[transport]
tcpMux = true
```

客户端：

```toml
# astrave.toml
server = "YOUR_SERVER_IP:9527"

[transport]
protocol = "tcp"
tcpMux = true
heartbeatInterval = -1
```

## 控制帧

客户端用 JSON 发送 `Login`。服务端可在 `LoginResp` 中返回 `codec = "binary"`，选择紧凑二进制编码。省略该字段时继续使用 JSON。帧头包含一个字节的消息标签和四个字节的小端正文长度，标签的最高位标识二进制编码。读取器拒绝未知标签和超过 4 MiB 的正文，UDP 读取器另设 128 KiB 帧上限。兼容性测试逐一核对旧六包实现生成的 12 种消息的 JSON 和二进制样本。

Windows 回环测试分别用旧客户端连接单包服务端、用单包客户端连接旧服务端。两种组合均完成了 16 次 32 KiB 回显校验。另用本版客户端和服务端配置私有 CA，分别经过 TCP、WebSocket、QUIC 和 KCP，每种传输均校验了四次 64 KiB 响应。

## 客户端参数

| 参数                            | 必填 | 默认值    | 说明                                       |
|-------------------------------|----|--------|------------------------------------------|
| `transport.protocol`          | 否  | `tcp`  | 固定为 `tcp`                                |
| `transport.tcpMux`            | 否  | `true` | TCP 多路复用；需与服务端一致                         |
| `transport.heartbeatInterval` | 否  | `-1`   | 应用心跳间隔（秒）；`-1` 关闭。关闭 `tcpMux` 时自动变为 `30` |
| `transport.heartbeatTimeout` | 否  | `-1` | 心跳超时（秒）；未收到 Pong 则断开。关闭 `tcpMux` 时自动变为 `90`；详见 [TCP 多路复用](../tcp-mux.md) |

## 服务端参数

| 参数                 | 必填 | 默认值              | 说明               |
|--------------------|----|------------------|------------------|
| `listen`           | 是  | `0.0.0.0:9527`   | TCP 监听地址         |
| `transport.tcpMux` | 否  | `true`           | TCP 多路复用；需与客户端一致 |
