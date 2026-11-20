---
sidebar_position: 2
sidebar_label: Docker
title: Docker
---

# Docker 安装

先运行 `make docker-server docker-client` 构建本地镜像。

## 服务端

在宿主机分别设置管理密码 `DASHBOARD_PASSWORD` 和客户端令牌 `ASTRAVE_TOKEN`。将服务端证书链和私钥放在 `./certs/fullchain.pem` 和 `./certs/server.key`。客户端连接的域名须在证书中。以下示例把隧道的 9000 端口发布到宿主机回环地址。

```toml
listen = "0.0.0.0:9527"

# 可选 通过域名路由
# httpGwPort = 80
# httpsGwPort = 443

[auth]
token = "{{env.ASTRAVE_TOKEN}}"

[transport.tls]
force = true
certFile = "/certs/fullchain.pem"
keyFile = "/certs/server.key"

# 可选：Web 管理面板
[dashboard]
addr = "0.0.0.0"
port = 8020
user = "admin"
password = "{{env.DASHBOARD_PASSWORD}}"
```

> `dashboard.addr` 需为 `0.0.0.0`，否则宿主机无法通过端口映射访问管理面板
### 方式一：配置文件启动

```shell
docker run -d --name astrave-server --restart unless-stopped \
  -p 9527:9527 -p 127.0.0.1:9000:9000 -e ASTRAVE_TOKEN \
  -v "$PWD/certs:/certs:ro" \
  -e DASHBOARD_PASSWORD -p 127.0.0.1:8020:8020 \
  -v "$PWD/astrave-server.toml:/etc/astrave/astrave-server.toml:ro" \
  astrave-server:local
```

### 方式二：Compose启动

```yaml
# docker-compose.yaml
services:
  astrave-server:
    image: astrave-server:local
    container_name: astrave-server
    restart: unless-stopped
    environment:
      DASHBOARD_PASSWORD: ${DASHBOARD_PASSWORD:?required}
      ASTRAVE_TOKEN: ${ASTRAVE_TOKEN:?required}
    ports:
      - "9527:9527"
      - "127.0.0.1:8020:8020"
      # - "80:80"
      # - "443:443"
      - "127.0.0.1:9000:9000"
    volumes:
      - ./certs:/certs:ro
      - ./astrave-server.toml:/etc/astrave/astrave-server.toml:ro
```

```shell
docker compose up -d
```

<div id="env">

### 方式三：用环境变量注入配置

</div>

配置里写 `{{env.NAME}}`，用 `-e` 或 Compose `environment` 传入。语法见 [环境变量](../features/env.md)。

```toml
#astrave-server.toml
listen = "{{env.ASTRAVE_LISTEN:0.0.0.0:9527}}"

[transport.tls]
force = true
certFile = "/certs/fullchain.pem"
keyFile = "/certs/server.key"

[auth]
token = "{{env.ASTRAVE_TOKEN}}"

[dashboard]
addr = "0.0.0.0"
port = 8020
user = "{{env.DASHBOARD_USER:admin}}"
password = "{{env.DASHBOARD_PASSWORD}}"
```

```yaml
# docker-compose.yaml
services:
  astrave-server:
    image: astrave-server:local
    container_name: astrave-server
    restart: unless-stopped
    ports:
      - "9527:9527"
      - "127.0.0.1:8020:8020"
      - "127.0.0.1:9000:9000"
    environment:
      ASTRAVE_TOKEN: ${ASTRAVE_TOKEN:?required}
      DASHBOARD_PASSWORD: ${DASHBOARD_PASSWORD:?required}
    volumes:
      - ./certs:/certs:ro
      - ./astrave-server.toml:/etc/astrave/astrave-server.toml:ro
```

```shell
docker compose up -d
```

> 字符串字段的占位符需要加引号。必填变量未设置时，进程会启动失败。
---

## 客户端

### 挂载配置

准备 `astrave.toml`：

```toml
server = "astrave.example.com:9527"

[auth]
token = "{{env.ASTRAVE_TOKEN}}"

[[tunnels]]
name = "mysql"
protocol = "tcp"
service = "127.0.0.1:3306"
remotePort = 9000
```

### 方式一：配置文件启动

```shell
docker run -d --name astrave --restart unless-stopped \
  -e ASTRAVE_TOKEN \
  -v "$PWD/astrave.toml:/etc/astrave/astrave.toml:ro" \
  astrave:local
```

> 容器内 `127.0.0.1` 是容器自己。若要穿透**宿主机**上的服务，把 `service` 改成宿主机 IP，或使用下方 host 网络
**host 网络**

与宿主机共用网络，配置里可直接写 `127.0.0.1` 访问本机服务：

```shell
docker run -d --name astrave --restart unless-stopped \
  --network host \
  -e ASTRAVE_TOKEN \
  -v "$PWD/astrave.toml:/etc/astrave/astrave.toml:ro" \
  astrave:local
```

### 方式二：Compose启动

```yaml
# docker-compose.yaml
services:
  astrave:
    image: astrave:local
    container_name: astrave
    restart: unless-stopped
    # network_mode: host
    environment:
      ASTRAVE_TOKEN: ${ASTRAVE_TOKEN:?required}
    volumes:
      - ./astrave.toml:/etc/astrave/astrave.toml:ro
```

```shell
docker compose up -d
```
