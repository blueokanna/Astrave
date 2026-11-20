---
sidebar_position: 4
sidebar_label: Docker
title: Docker
---

# Docker

Build the local images with `make docker-server docker-client` before starting the containers.

## Server

Set `DASHBOARD_PASSWORD` and `ASTRAVE_TOKEN` to separate secrets in the host environment. Place the server's certificate chain and key in `./certs/fullchain.pem` and `./certs/server.key`. Use a server name covered by the certificate. The commands publish the example tunnel on host loopback port 9000.

```toml
listen = "0.0.0.0:9527"

# optional: domain routing
# httpGwPort = 80
# httpsGwPort = 443

[auth]
token = "{{env.ASTRAVE_TOKEN}}"

[transport.tls]
force = true
certFile = "/certs/fullchain.pem"
keyFile = "/certs/server.key"

# optional: web dashboard
[dashboard]
addr = "0.0.0.0"
port = 8020
user = "admin"
password = "{{env.DASHBOARD_PASSWORD}}"
```

> `dashboard.addr` must be `0.0.0.0`, otherwise the host cannot reach the dashboard through port mapping
### Option 1: Start with a config file

```shell
docker run -d --name astrave-server --restart unless-stopped \
  -p 9527:9527 -p 127.0.0.1:9000:9000 -e ASTRAVE_TOKEN \
  -v "$PWD/certs:/certs:ro" \
  -e DASHBOARD_PASSWORD -p 127.0.0.1:8020:8020 \
  -v "$PWD/astrave-server.toml:/etc/astrave/astrave-server.toml:ro" \
  astrave-server:local
```

### Option 2: Start with Compose

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

### Option 3: Inject config from environment variables

</div>

Write `{{env.NAME}}` in the config, then pass values with `-e` or Compose `environment`. See [Environment Variables](../features/env.md) for the syntax.

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

> Enclose placeholders for string fields in quotes. Startup fails if a required variable is unset.
---

## Client

### Mount the config

Prepare `astrave.toml`:

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

### Option 1: Start with a config file

```shell
docker run -d --name astrave --restart unless-stopped \
  -e ASTRAVE_TOKEN \
  -v "$PWD/astrave.toml:/etc/astrave/astrave.toml:ro" \
  astrave:local
```

> Inside the container, `127.0.0.1` is the container itself. To expose a service on the **host**, set `service` to the host IP, or use host networking below
**Host networking**

Share the host network so the config can use `127.0.0.1` to reach local services:

```shell
docker run -d --name astrave --restart unless-stopped \
  --network host \
  -e ASTRAVE_TOKEN \
  -v "$PWD/astrave.toml:/etc/astrave/astrave.toml:ro" \
  astrave:local
```

### Option 2: Start with Compose

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
