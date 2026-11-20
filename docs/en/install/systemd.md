---
sidebar_position: 1
sidebar_label: systemd
title: systemd
---

## Linux

This guide walks through setting up **astrave-server** as a systemd service on Linux.

1. Install systemd

```shell
# CentOS/RHEL
yum install systemd

# Debian/Ubuntu
apt install systemd
```
2. Create the astrave-server service unit

```toml
sudo tee /etc/systemd/system/astrave-server.service > /dev/null << 'EOF'
[Unit]
Description = astrave server
After = network.target syslog.target
Wants = network.target

[Service]
Type = simple
# Replace with the actual paths on your system
ExecStart = /path/to/astrave-server -c /path/to/astrave-server.toml

[Install]
WantedBy = multi-user.target
EOF
```

3. Enable the service to start on boot

```shell
sudo systemctl enable astrave-server
```

> 1. Start astrave-server
> 
> ```shell
> sudo systemctl start astrave-server
> ```
> 
> 2. Stop astrave-server
> ```shell
> sudo systemctl stop astrave-server
> ```
> 3. Restart astrave-server
> ```shell
> sudo systemctl restart astrave-server
> ```
> 4. Check astrave-server status
> ```shell
> sudo systemctl status astrave-server
> ```