---
sidebar_position: 6
sidebar_label: 环境变量
title: 环境变量
---

# 环境变量

配置文件可用 `{{env.NAME}}` 引用进程环境变量，**Docker**、**systemd**、**K8s** 等凡能注入环境变量的场景都适用。

Flutter 桌面端在校验配置时读取自身进程的环境变量。请在启动桌面端之前设置必填变量。

## 语法

```toml
[auth]
token = "{{env.ASTRAVE_TOKEN}}"

[[tunnels]]
name = "ssh"
protocol = "tcp"
service = "127.0.0.1:22"
remotePort = {{ env.SSH_REMOTE_PORT:9000 }}
```

| 写法                                    | 含义               |
|---------------------------------------|------------------|
| `{{env.NAME}}`         | 必填。变量未设置或为空时启动失败 |
| `{{env.NAME:default}}` | 可选。未设置或为空时使用默认值  |
| `"{{env.NAME}}"`       | 字符串字段，占位符必须放在引号内 |
| `{{env.NAME}}`         | 数字、布尔字段，不要加引号    |

变量名只能包含字母、数字、下划线，且不能以数字开头。花括号内空白可有可无：`{{ env.NAME }}` 合法。

第一个 `:` 才是默认值分隔符，因此地址类默认值可以直接写：

```toml
listen = "{{env.ASTRAVE_LISTEN:0.0.0.0:9527}}"
server = "{{env.ASTRAVE_SERVER:127.0.0.1:9527}}"
```

密钥建议用必填写法 `{{env.ASTRAVE_TOKEN}}`，不要给 token 写一个可猜测的默认值。

