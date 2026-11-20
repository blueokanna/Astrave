---
sidebar_position: 2
sidebar_label: 管理界面
title: 管理界面
---

# 管理界面

服务端 Web 管理面板。 `port > 0` 时启用，启用后 **必须** 配置 `user` 与
`password`。


## 示例

```toml
# astrave-server.toml
listen = "0.0.0.0:9527"

[dashboard]
addr = "127.0.0.1"
port = 8020
user = "admin"
password = "{{env.ASTRAVE_DASHBOARD_PASSWORD}}"
```

启动服务端前设置 `ASTRAVE_DASHBOARD_PASSWORD`。浏览器打开 `http://127.0.0.1:8020`，输入用户名和密码。概览显示活动连接和近 24 小时或 7 天的流量，点击客户端或隧道可查看详情。客户端详情页支持确认后断开连接。

远程管理时，在反向代理上启用 HTTPS，再转发到回环监听地址。代理应保留原始 Host 请求头，以便管理表单校验来源。

## 参数

| 参数                    | 必填 | 默认值         | 说明                   |
|-----------------------|----|-------------|----------------------|
| `dashboard.addr`      | 否  | `127.0.0.1` | 管理面板监听地址 |
| `dashboard.port`      | 是  | `0`         | 监听端口；`0` 表示关闭        |
| `dashboard.user`      | 是  |             | 登录用户名                |
| `dashboard.password`  | 是  |             | 登录密码                 |
