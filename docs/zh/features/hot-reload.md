---
sidebar_position: 7
sidebar_label: 热重载
title: 热重载
---

# 热重载

客户端运行期间修改配置并保存后，可让变更立即生效，**无需重启客户端**

- **热重载**：控制连接保持在线，只增删改隧道注册
- **重连**：断开控制连接后自动重连，期间全部隧道短暂中断

> 仅**隧道配置**变更走热重载，连接级字段（`server`、`user`、`auth`、`transport`、`udpPacketSize`
> ）变更会触发重连
## 用法

先以前台方式启动客户端（会监听本地控制套接字）：

```shell
astrave -c conf/astrave.toml
```

另开终端，对同一配置文件执行 reload：

```shell
astrave reload -c conf/astrave.toml
```

> `astrave reload` 通过本地控制通道与运行中的 `astrave -c` 进程通信（Unix 套接字 / Windows Named Pipe）
校验配置语法与字段（不连接服务端、不修改运行状态）：

```shell
astrave verify -c conf/astrave.toml
```

建议先 `verify` 再 `reload`：

```shell
astrave verify -c conf/astrave.toml && astrave reload -c conf/astrave.toml
```

