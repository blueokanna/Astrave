---
sidebar_position: 3
title: Quick Start
---

# Local quick start

Run each command in a separate terminal in the repository root. The first starts the Rust documentation preview on port 8000.

```shell
cargo run --locked -p astrave-docs -- --serve
```

```shell
cargo run --locked -p astrave-server -- -c conf/astrave-server.toml
```

```shell
cargo run --locked -p astrave-client -- -c conf/astrave.toml
```

Open `http://127.0.0.1:9000` to read the documentation through the tunnel.
