---
sidebar_position: 89
sidebar_label: Product comparison
---

# Choosing an access path for a private service

Last reviewed on 23 November 2026. Orbien refers to the Rust project `orbien-org/orbien`. Peer capabilities below come from upstream documentation and versioned source.

| Product | Access path | Access control and operations |
| --- | --- | --- |
| Astrave | A client dials a self-hosted gateway that publishes TCP and UDP ports or routes HTTP and HTTPS by host. Paired clients can also reach a configured TCP service over direct QUIC. | Token, private CA, mTLS, SOCKS5 plugin and hot reload. The client and gateway display their own resident memory. |
| ngrok | The agent connects to a managed endpoint that forwards HTTP, TLS or TCP traffic. Public TCP endpoints can use a reserved address. | ngrok operates the public ingress. Traffic Policy controls access and the dashboard records connections. An account was unavailable for a matched throughput test. [TCP Agent Endpoints](https://ngrok.com/docs/universal-gateway/tcp) |
| frp 0.72.0 | `frpc` connects to `frps` for public ports and virtual hosts. STCP/SUDP support visitor access. XTCP attempts a peer-to-peer connection. | Token or OIDC, client and server web interfaces, load balancing and health checks. [Features](https://gofrp.org/en/docs/features/) · [Release](https://github.com/fatedier/frp/releases/tag/v0.72.0) |
| Orbien 3.10.0 | A self-hosted gateway publishes TCP, UDP, HTTP, HTTPS and SOCKS5 over TCP, KCP, WebSocket or QUIC. | Upstream lists multiplexing, Token, TLS/mTLS, hot reload, desktop management and ACME. Its gateway model closely matches Astrave’s. [Versioned README](https://github.com/orbien-org/orbien/blob/v3.10.0/README.md) |
| Tailscale 1.104.7 | Tailnet devices communicate over WireGuard, directly when possible or via a peer relay or DERP. Funnel publishes a service to internet visitors. | Device identity and tailnet policies control access. Record the actual path with `tailscale ping` for a private-link benchmark and measure Funnel separately. [Connection types](https://tailscale.com/kb/1257/connection-types) · [Funnel source](https://github.com/tailscale/tailscale/blob/v1.104.7/cmd/tailscale/cli/funnel.go) |

## Loopback measurements

On 23 November, the same driver compared the current Windows x64 workspace release with Orbien. Median duplex throughput was **313.4 MiB/s** (256.7–315.7) for Astrave and **280.3 MiB/s** (225.6–323.0) for Orbien. Median gateway restart to a verified new connection was **1.025 s** and **1.522 s** respectively. The throughput ranges overlap. The tables below retain the earlier build measurements. [23 November raw samples](https://github.com/blueokanna/Astrave/blob/main/docs/benchmarks/peers-windows-2026-11-23.jsonl)

Each Windows x64 trial used one TCP echo origin, token authentication, multiplexing and transport TLS disabled. Eight warmed connections made 128 round trips of 64 KiB each, verifying 128 MiB of duplex traffic. The driver then killed the gateway and kept the client alive. Recovery timing runs from launching the replacement gateway to receiving the first verified 64 KiB echo over a new connection. Each product ran three interleaved trials.

| Product | Median duplex MiB/s (range) | Median restart to echo (range) |
| --- | ---: | ---: |
| Astrave | 281.7 (227.2–294.1) | 1.360 s (1.035–1.408) |
| Orbien | 257.5 (190.5–316.4) | 1.194 s (1.020–1.322) |

On a single Ubuntu 24.04 x64 GitHub runner, the same driver measured all three products with rotating trial order. All nine trials verified every byte before and after the gateway restart. The throughput ranges overlap. Recovery time includes process startup, the client's retry delay and polling at 100 ms intervals. [GitHub Actions run](https://github.com/blueokanna/Astrave/actions/runs/44858386871) · [Raw Linux samples](https://github.com/blueokanna/Astrave/blob/main/docs/benchmarks/peers-linux.jsonl)

| Linux gateway | Median duplex MiB/s (range) | Median restart to echo (range) |
| --- | ---: | ---: |
| Astrave | 119.68 (119.22–157.49) | 1.019 s (1.019–1.021) |
| Orbien | 120.33 (120.09–121.74) | 1.018 s (1.017–1.021) |
| frp | 120.09 (119.34–121.29) | 2.130 s (2.128–2.131) |

The Windows trials ran alongside local builds. Windows denied execution of frpc. The Linux workflow checks upstream release archive digests before measuring each reverse tunnel. [Raw Windows samples](https://github.com/blueokanna/Astrave/blob/main/docs/benchmarks/peers-windows.jsonl) · [Linux comparison runs](https://github.com/blueokanna/Astrave/actions/workflows/compare.yml)

On a separate Ubuntu runner, upstream Tailscale v1.104.7 started two `tailscaled` processes and a private test coordination server. Peer discovery and IPv4/IPv6 ping passed with and without the test server's address masquerading. A three-node case disabled direct UDP and verified six directed peer-relay paths in daemon status. These are connectivity checks. Production NAT traversal, DERP throughput and public Funnel access remain unmeasured. [Execution log](https://github.com/blueokanna/Astrave/actions/runs/44859128175/job/136708181563) · [Test source](https://github.com/tailscale/tailscale/blob/v1.104.7/tstest/integration/integration_test.go)

```sh
dart scripts/compare-tunnels.dart astrave=target/release orbien=/path/to/orbien frp=/path/to/frp
```

The Astrave integration test also holds an accepted socket open without answering the handshake and checks that the same client can reconnect to a working gateway. Separate KCP and QUIC cases verify echoes under periodic bidirectional loss. Internet NAT behavior, long-running sessions and mobile network handoffs require deployment measurements. [Integration test](https://github.com/blueokanna/Astrave/blob/main/scripts/check-tunnels.py)
