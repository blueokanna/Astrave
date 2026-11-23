---
sidebar_position: 2
sidebar_label: WebSocket
title: WebSocket
---

# WebSocket

Shares the server `listen` address with TCP. Useful when you need to traverse an HTTP proxy, or when only WebSocket is
allowed. For TLS, see [TLS](../tls.md).

## Example

Server:

```toml
# astrave-server.toml
listen = "0.0.0.0:9527"

[transport]
tcpMux = true
wsPath = "/ws"
```

Client:

```toml
# astrave.toml
server = "YOUR_SERVER_IP:9527"

[transport]
protocol = "websocket"
tcpMux = true
heartbeatInterval = -1
wsPath = "/ws"
```

## Client parameters

| Parameter                     | Required | Default     | Description                                                                                                                  |
|-------------------------------|----------|-------------|------------------------------------------------------------------------------------------------------------------------------|
| `transport.protocol`          | Yes      | `tcp`       | `websocket` (or `ws`) for direct connections, `wss` for an HTTPS reverse proxy |
| `transport.wsTrustedCaFile`   | No       | System roots | PEM trust roots for a private WSS edge. Relative paths resolve from the client configuration directory. |
| `transport.wsHeaderEnv`       | No       | Empty | Maps WSS edge header names to environment variable names. [Authenticated edge configuration](../../cloud.md) |
| `transport.wsPath`            | No       | `/~!astrave` | WebSocket HTTP path; must match the server                                                                                   |
| `transport.tcpMux`            | No       | `true`      | TCP multiplexing; must match the server                                                                                      |
| `transport.heartbeatInterval` | No       | `-1`        | Application heartbeat interval (seconds); `-1` disables it. Becomes `30` when `tcpMux` is off                                |
| `transport.heartbeatTimeout`  | No       | `-1`        | Heartbeat timeout (seconds); disconnect if no Pong. Becomes `90` when `tcpMux` is off; see [TCP Multiplexing](../tcp-mux.md) |

## Server parameters

| Parameter          | Required | Default        | Description                                |
|--------------------|----------|----------------|--------------------------------------------|
| `listen`           | Yes      | `0.0.0.0:9527` | Shared listen address with TCP             |
| `transport.wsPath` | No       | `/~!astrave`    | WebSocket HTTP path; must match the client |
| `transport.tcpMux` | No       | `true`         | TCP multiplexing; must match the client    |

## Connect to the gateway through Cloudflare Tunnel

Run `cloudflared` on the same host as `astrave-server`. The client verifies the Cloudflare edge against the hostname in `server`. After the WebSocket upgrade it verifies the Astrave origin against `transport.tls.serverName`. The client presents its certificate only during the inner mTLS handshake. This route carries the agent-to-gateway connection. Published service addresses follow the gateway configuration.

For a locally managed named tunnel, enable WebSockets in the zone and route `edge.example.com` to the tunnel in DNS. Substitute your own hostnames, tunnel UUID and certificate paths.

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
# astrave.toml, followed by your existing [[tunnels]] entries
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

Include `origin.internal.example` in the origin certificate's DNS SAN and set the same `ASTRAVE_TOKEN` for both Astrave processes. Validate the route, start the tunnel, then start the agent. The agent reconnects after an edge disconnect. Applications using a stream at the time of the disconnect need to open a new connection.

```sh
cloudflared tunnel ingress validate
cloudflared tunnel ingress rule https://edge.example.com/ws
cloudflared tunnel run <TUNNEL-UUID>
```

For a hostname protected by Cloudflare Access, authorize the service token with a **Service Auth** policy. Add the following table to the Astrave client configuration and set both environment variables for the client process. It sends the service token on each WSS upgrade, including reconnects. [Cloudflare service tokens](https://developers.cloudflare.com/cloudflare-one/access-controls/service-credentials/service-tokens/)

```toml
[transport.cloudflareAccess]
clientId = "{{env.CF_ACCESS_CLIENT_ID}}"
clientSecret = "{{env.CF_ACCESS_CLIENT_SECRET}}"
```

[Cloudflare ingress configuration](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/local-management/configuration-file/) · [Cloudflare WebSockets](https://developers.cloudflare.com/network/websockets/)
