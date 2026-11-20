---
sidebar_position: 3
sidebar_label: TLS
title: TLS
---

# TLS

Control-channel TLS. Applies to `tcp` / `websocket` / `kcp` (QUIC is already encrypted, but the certificate path fields can still be used for QUIC identity verification).

The client verifies the server name and certificate chain whenever TLS is enabled.

1. **System trust store** (default)
2. **Private CA** supplied by the operator
3. **Mutual TLS (mTLS)**: Both sides verify each other's certificates

The control channel uses a standard TLS ClientHello (first byte `0x16`). HTTP / HTTPS domain gateways must use separate ports; see [Domains](../tunnels/domains.md).

## System trust store

Leave the client’s `trustedCaFile` unset to use the host trust store. Configure a server certificate covering the name in `server` or `transport.tls.serverName`. A temporary self-signed server certificate fails verification unless explicitly trusted.

Server:

```toml
# astrave-server.toml
[transport.tls]
force = true
certFile = "/path/to/fullchain.pem"
keyFile = "/path/to/server.key"
```

Client:

```toml
# astrave.toml
[transport.tls]
enable = true
```

## Private CA

The server provides a certificate; the client verifies it with a CA and sets SNI.

Server:

```toml
# astrave-server.toml
[transport.tls]
force = true
certFile = "/path/to/server.crt"
keyFile = "/path/to/server.key"
```

Client:

```toml
# astrave.toml
[transport.tls]
enable = true
trustedCaFile = "/path/to/ca.crt"
serverName = "astrave.example.com"
```

## Mode 3: Mutual TLS (mTLS)

Both sides verify the peer certificate. Setting `trustedCaFile` on the server automatically implies `force = true`.

Server:

```toml
# astrave-server.toml
[transport.tls]
force = true
certFile = "/path/to/server.crt"
keyFile = "/path/to/server.key"
trustedCaFile = "/path/to/ca.crt"
```

Client:

```toml
# astrave.toml
[transport.tls]
enable = true
certFile = "/path/to/client.crt"
keyFile = "/path/to/client.key"
trustedCaFile = "/path/to/ca.crt"
serverName = "astrave.example.com"
```

## Client parameters

| Parameter                     | Required | Default | Description                                                          |
|-------------------------------|----------|---------|----------------------------------------------------------------------|
| `transport.tls.enable`        | No       | `true`  | Enable TLS; ignored for QUIC, which is already encrypted             |
| `transport.tls.certFile`      | No       |         | Client certificate (mTLS)                                            |
| `transport.tls.keyFile`       | No       |         | Client private key (mTLS)                                            |
| `transport.tls.trustedCaFile` | No       |         | Private trust bundle in PEM format. An empty value selects the system trust store. |
| `transport.tls.serverName`    | No       |         | TLS SNI; empty uses the host part of `server`                        |

## Server parameters

| Parameter                     | Required | Default | Description                                                          |
|-------------------------------|----------|---------|----------------------------------------------------------------------|
| `transport.tls.force`         | No       | `false` | Require TLS; reject non-TLS control connections                      |
| `transport.tls.certFile`      | No       |         | Server certificate; empty uses a temporary self-signed cert          |
| `transport.tls.keyFile`       | No       |         | Server private key; empty uses a temporary self-signed cert          |
| `transport.tls.trustedCaFile` | No       |         | Verify the client certificate (mTLS); non-empty automatically sets `force = true` |
