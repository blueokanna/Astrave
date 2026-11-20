---
sidebar_position: 7
sidebar_label: Hot Reload
title: Hot Reload
---

# Hot Reload

While the client is running, saving the config applies changes immediately, **without restarting the client**.

- **Hot reload**: Keeps the control connection up; adds, removes, or updates tunnel registrations only.
- **Reconnect**: Drops the control connection and reconnects automatically; all tunnels are briefly interrupted.

> Only **tunnel** changes use hot reload. Changes to connection-level fields (`server`, `user`, `auth`, `transport`, `udpPacketSize`) trigger a reconnect.
## Usage

Start the client in the foreground (this opens the local control socket):

```shell
astrave -c conf/astrave.toml
```

In another terminal, reload the same config file:

```shell
astrave reload -c conf/astrave.toml
```

> `astrave reload` talks to the running `astrave -c` process over a local control channel (Unix socket / Windows named pipe).
Validate syntax and fields without connecting to the server or changing the running state:

```shell
astrave verify -c conf/astrave.toml
```

Run `verify` before `reload`:

```shell
astrave verify -c conf/astrave.toml && astrave reload -c conf/astrave.toml
```
