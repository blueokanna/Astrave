---
sidebar_position: 1
sidebar_label: Authentication
title: Authentication
---

# Authentication

Token authentication when the client connects to the server. If the server has no `token` (or it is empty), authentication is skipped.

When the server enables auth, both sides must use the same `token`, or login fails. The token is read from the config file. You can also inject it with environment variables; see [Environment Variables](./env.md).

> At login the token is used to compute a digest. The token itself is never sent to the server in plaintext.
## Example

Server:

```toml
# astrave-server.toml
[auth]
token = "YOUR_TOKEN"
```

Client:

```toml
# astrave.toml
[auth]
token = "YOUR_TOKEN"
```

## Parameters

| Parameter    | Required | Default | Description                                                          |
|--------------|----------|---------|----------------------------------------------------------------------|
| `auth.token` | No       |         | Shared secret; empty on the server disables auth; both sides must match |
