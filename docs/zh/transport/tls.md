---
sidebar_position: 3
sidebar_label: TLS
title: TLS
---

# TLS

控制通道 TLS。适用于 `tcp` / `websocket` / `kcp`（QUIC 自身已加密，但证书路径字段仍可用于 QUIC 身份校验）。

启用 TLS 后，客户端校验服务端名称和证书链。

1. **系统信任库**（默认）
2. **私有 CA**，由部署者指定
3. **双向认证（mTLS）**：双方互验证书

控制通道使用标准 TLS ClientHello（首字节 `0x16`）。HTTP / HTTPS 域名入口必须使用独立端口，见 [域名](../tunnels/domains.md)。

## 系统信任库

客户端将 `trustedCaFile` 留空时使用系统信任库。服务端证书须覆盖 `server` 中的主机名或显式设置的 `transport.tls.serverName`。临时自签证书需要单独配置信任才可通过校验。

服务端：

```toml
# astrave-server.toml
[transport.tls]
force = true
certFile = "/path/to/fullchain.pem"
keyFile = "/path/to/server.key"
```

客户端：

```toml
# astrave.toml
[transport.tls]
enable = true
```

## 私有 CA

服务端提供证书；客户端用 CA 校验，并设置 SNI。

服务端：

```toml
# astrave-server.toml
[transport.tls]
force = true
certFile = "/path/to/server.crt"
keyFile = "/path/to/server.key"
```

客户端：

```toml
# astrave.toml
[transport.tls]
enable = true
trustedCaFile = "/path/to/ca.crt"
serverName = "astrave.example.com"
```

## 方式三：双向认证（mTLS）

双方均校验对方证书。服务端配置 `trustedCaFile` 时会自动 `force = true`。

服务端：

```toml
# astrave-server.toml
[transport.tls]
force = true
certFile = "/path/to/server.crt"
keyFile = "/path/to/server.key"
trustedCaFile = "/path/to/ca.crt"
```

客户端：

```toml
# astrave.toml
[transport.tls]
enable = true
certFile = "/path/to/client.crt"
keyFile = "/path/to/client.key"
trustedCaFile = "/path/to/ca.crt"
serverName = "astrave.example.com"
```

## 客户端参数

| 参数                            | 必填 | 默认值    | 说明                                  |
|-------------------------------|----|--------|-------------------------------------|
| `transport.tls.enable`        | 否  | `true` | 启用 TLS；QUIC 本身已加密，此项无效              |
| `transport.tls.certFile`      | 否  |        | 客户端证书（mTLS）                         |
| `transport.tls.keyFile`       | 否  |        | 客户端私钥（mTLS）                         |
| `transport.tls.trustedCaFile` | 否  |        | PEM 格式的私有信任证书包。留空时采用系统信任库。 |
| `transport.tls.serverName`    | 否  |        | TLS SNI；空则使用 `server` 的主机部分         |

## 服务端参数

| 参数                            | 必填 | 默认值     | 说明                                 |
|-------------------------------|----|---------|------------------------------------|
| `transport.tls.force`         | 否  | `false` | 强制 TLS，拒绝非 TLS 控制连接                |
| `transport.tls.certFile`      | 否  |         | 服务端证书；空则临时自签                       |
| `transport.tls.keyFile`       | 否  |         | 服务端私钥；空则临时自签                       |
| `transport.tls.trustedCaFile` | 否  |         | 校验客户端证书（mTLS）；非空时自动 `force = true` |
