---
sidebar_position: 1
sidebar_label: TCP
title: TCP
---

# TCP

The default transport protocol. The client establishes a control channel through the server `listen` address. For TLS, see [TLS](../tls.md).

## Example

Server:

```toml
# astrave-server.toml
listen = "0.0.0.0:9527"

[transport]
tcpMux = true
```

Client:

```toml
# astrave.toml
server = "YOUR_SERVER_IP:9527"

[transport]
protocol = "tcp"
tcpMux = true
heartbeatInterval = -1
```

## Control frames

The client sends `Login` as JSON. A server can select the compact binary profile by returning `codec = "binary"` in `LoginResp`. An absent codec keeps the session on JSON. Each frame carries a one-byte message tag and a four-byte little-endian body length. The high bit of the tag selects binary. The reader rejects unknown tags and bodies above 4 MiB. UDP readers enforce a smaller 128 KiB frame limit. The compatibility tests check all 12 message types against JSON and binary fixtures produced by the six-package implementation.

In a Windows loopback test, the old client connected to the single-package server and the single-package client connected to the old server. Each pairing verified 16 echoes of 32 KiB. Separate tests of the current client and server verified four 64 KiB responses over each of TCP, WebSocket, QUIC and KCP using a private CA.

## Client parameters

| Parameter                     | Required | Default | Description                                                                                          |
|-------------------------------|----------|---------|------------------------------------------------------------------------------------------------------|
| `transport.protocol`          | No       | `tcp`   | Always `tcp`                                                                                         |
| `transport.tcpMux`            | No       | `true`  | TCP multiplexing; must match the server                                                              |
| `transport.heartbeatInterval` | No       | `-1`    | Application heartbeat interval (seconds); `-1` disables it. Becomes `30` when `tcpMux` is off        |
| `transport.heartbeatTimeout`  | No       | `-1`    | Heartbeat timeout (seconds); disconnect if no Pong. Becomes `90` when `tcpMux` is off; see [TCP Multiplexing](../tcp-mux.md) |

## Server parameters

| Parameter          | Required | Default          | Description                          |
|--------------------|----------|------------------|--------------------------------------|
| `listen`           | Yes      | `0.0.0.0:9527`   | TCP listen address                   |
| `transport.tcpMux` | No       | `true`           | TCP multiplexing; must match the client |
