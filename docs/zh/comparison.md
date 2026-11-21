---
sidebar_position: 89
sidebar_label: 竞品对比
---

# 内网服务该怎样对外提供访问？

核对日期为 2026 年 11 月 21 日。Orbien 采用 Rust 项目 `orbien-org/orbien`。下表的竞品能力来自对应版本的上游文档和代码。

| 产品 | 访问路径 | 访问控制与运维 |
| --- | --- | --- |
| Astrave 1.0.0 候选版 | 客户端主动连接自建网关，由网关公开 TCP、UDP 或按域名转发 HTTP、HTTPS。SOCKS5 可通过客户端插件提供。 | Token、私有 CA 和 mTLS，支持 HTTPS 透传及客户端终止；Web 面板查看隧道，Flutter 客户端管理配置和证书。 |
| frp 0.72.0 | `frpc` 连到 `frps` 做公网端口或虚拟主机映射。STCP/SUDP 提供访客模式，XTCP 尝试建立点对点连接。 | 支持 Token 和 OIDC、客户端及服务端 Web 管理、负载均衡和健康检查。[功能文档](https://gofrp.org/zh-cn/docs/features/) · [版本](https://github.com/fatedier/frp/releases/tag/v0.72.0) |
| Orbien 3.10.0 | 自建网关发布 TCP、UDP、HTTP、HTTPS 和 SOCKS5。可选 TCP、KCP、WebSocket 或 QUIC 传输。 | 上游列有多路复用、Token、TLS/mTLS、热重载、桌面端和 ACME。它与 Astrave 的公网转发模式最接近。[版本文档](https://github.com/orbien-org/orbien/blob/v3.10.0/README_ZH.md) |
| Tailscale 1.104.7 | 加入 tailnet 的设备通过 WireGuard 互通。可直连时数据走设备间路径，也可经 Peer Relay 或 DERP 中继。向无客户端的公网访客提供服务可启用 Funnel。 | 以设备身份和 tailnet 访问策略授权；评估性能时需要记录 `tailscale ping` 实际选中的路径，Funnel 另测。[连接类型](https://tailscale.com/kb/1257/connection-types) · [Funnel 实现](https://github.com/tailscale/tailscale/blob/v1.104.7/cmd/tailscale/cli/funnel.go) |

## 同机实测

Windows x64 上使用同一 TCP 回显源、Token、TCP 多路复用，关闭传输 TLS。每轮预热 8 条连接，每条往返 128 次，每次 64 KiB，每轮共校验 128 MiB 双向流量。随后终止网关进程，保持原客户端运行，从启动新网关开始，计时到新连接首次收到完整的 64 KiB 回显。每版三轮交错执行。

| 产品 | 双向吞吐中位数（范围），MiB/s | 重启至回显中位数（范围） |
| --- | ---: | ---: |
| Astrave | 281.7（227.2–294.1） | 1.360 秒（1.035–1.408） |
| Orbien | 257.5（190.5–316.4） | 1.194 秒（1.020–1.322） |

两者均完成全部回显和重连，吞吐区间重叠。数据只覆盖本机回环，测试期间本机还有构建任务。Windows 拒绝运行 frpc，表中没有 Windows frp 样本。Tailscale 尚缺两台已授权的测试节点，直连、DERP 和 Funnel 的吞吐均未实测。三种反向隧道使用同一对比脚本，Linux 工作流校验上游发布包 SHA-256 后运行三轮并保存原始数据。[Windows 原始样本](https://github.com/blueokanna/Astrave/blob/main/docs/benchmarks/peers-windows.jsonl) · [Linux 对比运行](https://github.com/blueokanna/Astrave/actions/workflows/compare.yml)

```sh
dart scripts/compare-tunnels.dart astrave=target/release orbien=/path/to/orbien frp=/path/to/frp
```

Astrave 的故障恢复测试还会让监听端接收连接后保持沉默，确认同一客户端在握手超时后能连到真正的网关；独立的 KCP 和 QUIC 测试双向周期性丢包并逐字节校验。公网 NAT、长时间运行和手机网络切换需要在实际部署条件下测量。[测试代码](https://github.com/blueokanna/Astrave/blob/main/scripts/check-tunnels.py)
