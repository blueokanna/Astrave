---
sidebar_position: 3
title: 快速开始
---

# 本地快速开始

在项目根目录打开三个终端，每个终端执行一条命令。第一条命令在 8000 端口启动 Rust 文档预览服务。

```shell
cargo run --locked -p astrave-docs -- --serve
```

```shell
cargo run --locked -p astrave-server -- -c conf/astrave-server.toml
```

```shell
cargo run --locked -p astrave-client -- -c conf/astrave.toml
```

打开 `http://127.0.0.1:9000` 即可通过隧道阅读文档。
