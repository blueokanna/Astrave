---
sidebar_position: 1
sidebar_label: systemd
title: systemd
---

## Linux

下文介绍在 Linux 操作系统通过 systemd 安装 **astrave-server** 服务端。

1. 安装 systemd

```shell
# CentOS/RHEL
yum install systemd

# Debian/Ubuntu
apt install systemd
```
2. 创建 astrave-server 服务 

```toml
sudo tee /etc/systemd/system/astrave-server.service > /dev/null << 'EOF'
[Unit]
Description = astrave server
After = network.target syslog.target
Wants = network.target

[Service]
Type = simple
# 需要修改为实际路径
ExecStart = /path/to/astrave-server -c /path/to/astrave-server.toml

[Install]
WantedBy = multi-user.target
EOF
```

3. 设置开机自启动

```shell
sudo systemctl enable astrave-server
```

> 1. 启动 astrave-server
> 
> ```shell
> sudo systemctl start astrave-server
> ```
> 
> 2. 停止 astrave-server
> ```shell
> sudo systemctl stop astrave-server
> ```
> 3. 重启 astrave-server
> ```shell
> sudo systemctl restart astrave-server
> ```
> 4. 查看 astrave-server 状态
> ```shell
> sudo systemctl status astrave-server
> ```