---
sidebar_position: 2
sidebar_label: Dashboard
title: Dashboard
---

# Dashboard

Server web dashboard. Enabled when `port > 0`; **`user` and `password` are required** once enabled.


## Example

```toml
# astrave-server.toml
listen = "0.0.0.0:9527"

[dashboard]
addr = "127.0.0.1"
port = 8020
user = "admin"
password = "{{env.ASTRAVE_DASHBOARD_PASSWORD}}"
```

Set `ASTRAVE_DASHBOARD_PASSWORD` before starting the server. Open `http://127.0.0.1:8020` and sign in. Overview shows active connections and traffic over 24 hours or 7 days. Select a client or tunnel for details. The client page includes the disconnect action.

For remote access, terminate HTTPS at a reverse proxy and forward requests to the loopback listener. Preserve the original Host header for dashboard forms.

## Parameters

| Parameter               | Required | Default       | Description                                  |
|-------------------------|----------|---------------|----------------------------------------------|
| `dashboard.addr`        | No       | `127.0.0.1`   | Dashboard listen address |
| `dashboard.port`        | Yes      | `0`           | Listen port; `0` disables the dashboard      |
| `dashboard.user`        | Yes      |               | Login username                               |
| `dashboard.password`    | Yes      |               | Login password                               |
