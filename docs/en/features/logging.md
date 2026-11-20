---
sidebar_position: 8
sidebar_label: Logging
title: Logging
---

# Logging

Logs can be persisted to the filesystem.

- If `[log]` is omitted or `file` is empty, logs go to the terminal only
- If `file` is set, logs are written to both the terminal and the file
- The file is rotated daily; backups older than `maxDays` are deleted

> Log settings take effect only at process startup and **do not support hot reload**.
## Example: Client

```toml
# astrave.toml
[log]
file = "./astrave.log"
level = "info"
maxDays = 3
```

## Example: Server

```toml
# astrave-server.toml
[log]
file = "./astrave-server.log"
level = "info"
maxDays = 3
```

## Parameters

| Parameter | Required | Default | Description                                          |
|-----------|----------|---------|------------------------------------------------------|
| `file`    | No       |         | Log file path; empty means logs are not persisted    |
| `level`   | No       | `info`  | `trace` / `debug` / `info` / `warn` / `error`        |
| `maxDays` | No       | `3`     | Days to keep rotated backups; `0` means never delete |
