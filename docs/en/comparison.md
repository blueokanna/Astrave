---
sidebar_position: 89
sidebar_label: Product comparison
---

# Choosing an access path for a private service

Reviewed on 21 November 2026. Orbien refers to the Rust project `orbien-org/orbien`. Peer capabilities below come from upstream documentation and versioned source.

| Product | Access path | Access control and operations |
| --- | --- | --- |
| Astrave 1.0.0 candidate | A client dials a self-hosted gateway that publishes TCP and UDP ports or routes HTTP and HTTPS by host. A client plugin provides SOCKS5. | Token, private CA, mTLS, HTTPS passthrough and client termination. The web dashboard lists tunnels and the Flutter client manages profiles and certificates. |
| frp 0.72.0 | `frpc` connects to `frps` for public ports and virtual hosts. STCP/SUDP support visitor access. XTCP attempts a peer-to-peer connection. | Token or OIDC, client and server web interfaces, load balancing and health checks. [Features](https://gofrp.org/en/docs/features/) · [Release](https://github.com/fatedier/frp/releases/tag/v0.72.0) |
| Orbien 3.10.0 | A self-hosted gateway publishes TCP, UDP, HTTP, HTTPS and SOCKS5 over TCP, KCP, WebSocket or QUIC. | Upstream lists multiplexing, Token, TLS/mTLS, hot reload, desktop management and ACME. Its gateway model closely matches Astrave’s. [Versioned README](https://github.com/orbien-org/orbien/blob/v3.10.0/README.md) |
| Tailscale 1.104.7 | Tailnet devices communicate over WireGuard, directly when possible or via a peer relay or DERP. Funnel publishes a service to internet visitors. | Device identity and tailnet policies control access. Record the actual path with `tailscale ping` for a private-link benchmark and measure Funnel separately. [Connection types](https://tailscale.com/kb/1257/connection-types) · [Funnel source](https://github.com/tailscale/tailscale/blob/v1.104.7/cmd/tailscale/cli/funnel.go) |

## Loopback measurements

Each Windows x64 trial used one TCP echo origin, token authentication, multiplexing and transport TLS disabled. Eight warmed connections made 128 round trips of 64 KiB each, verifying 128 MiB of duplex traffic. The driver then killed the gateway and kept the client alive. Recovery timing runs from launching the replacement gateway to receiving the first verified 64 KiB echo over a new connection. Each product ran three interleaved trials.

| Product | Median duplex MiB/s (range) | Median restart to echo (range) |
| --- | ---: | ---: |
| Astrave | 281.7 (227.2–294.1) | 1.360 s (1.035–1.408) |
| Orbien | 257.5 (190.5–316.4) | 1.194 s (1.020–1.322) |

Both clients recovered and verified every echo. Throughput ranges overlap. Builds were running on the same host during these loopback trials. Windows denied execution of frpc. Tailscale direct, DERP and Funnel measurements require authorized test nodes. The Linux workflow checks upstream release archive digests and records three trials per reverse tunnel with the same driver. [Raw Windows samples](https://github.com/blueokanna/Astrave/blob/main/docs/benchmarks/peers-windows.jsonl) · [Linux comparison runs](https://github.com/blueokanna/Astrave/actions/workflows/compare.yml)

```sh
dart scripts/compare-tunnels.dart astrave=target/release orbien=/path/to/orbien frp=/path/to/frp
```

The Astrave integration test also holds an accepted socket open without answering the handshake and checks that the same client can reconnect to a working gateway. Separate KCP and QUIC cases verify echoes under periodic bidirectional loss. Internet NAT behavior, long-running sessions and mobile network handoffs require deployment measurements. [Integration test](https://github.com/blueokanna/Astrave/blob/main/scripts/check-tunnels.py)
