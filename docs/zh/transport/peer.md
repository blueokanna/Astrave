# 两台主机之间的服务直连

`astrave rendezvous` 负责配对。两端用各自的 UDP 套接字，通过双向 TLS 向会合服务登记。会合服务交换它实际看到的地址和端口，发布端随即发送探测包，消费端从原套接字发起 QUIC 握手。建立直连后，业务流量在两端之间传输。此时停止会合服务，已有连接仍可继续工作。

为三个进程分别签发证书。会合服务需要 `serverAuth` 和与 `rendezvous_name` 相符的 DNS SAN。发布端同时需要 `clientAuth`、`serverAuth` 以及与 `peer_name` 相符的 DNS SAN。消费端需要 `clientAuth`。用 `openssl x509 -in peer.pem -noout -fingerprint -sha256` 读取叶证书指纹，去除冒号，得到配置所需的 64 位十六进制串。三端使用同一私有 CA 的公钥证书，CA 私钥留在签发端。

1. 在具备可达 UDP 端口的机器上运行 `astrave rendezvous -c config.toml`。`bind` 指定监听地址，`peer_sha256` 填两个端点各自的指纹。
2. 在运行 Magpie、sub2api 或其他 TCP 服务的机器上运行 `astrave peer -c config.toml`。选择 `role = "publish"`，`local` 填服务实际监听的回环地址，`peer_sha256` 填消费端的指纹。服务原有的 API 密钥和登录机制继续生效。
3. 在消费端运行同一条 peer 命令。选择 `role = "consume"`，`local` 填本机空闲的回环端口，指纹改为发布端指纹。两个端点的 `rendezvous` 均填会合服务实际的 IP 和 UDP 端口。将应用的上游地址指向消费端本机端口，即可使用这条连接。一个 QUIC 双向流对应一条 TCP 连接。

会合服务配置示例如下。将指纹占位符替换为对应叶证书的 64 位十六进制 SHA-256 指纹。三个进程都从 `[tls]` 读取各自的证书、私钥和 CA 证书，相对路径以进程工作目录为准。

```toml
bind = "0.0.0.0:7443"
peer_sha256 = ["PUBLISHER_SHA256", "CONSUMER_SHA256"]
[tls]
cert = "rendezvous.pem"
key = "rendezvous.key"
ca = "ca.pem"
```

若发布端本机的服务监听 8080 端口，端点配置可以这样填写。将 `192.0.2.1` 换成会合服务的实际 IP，两个名称换成对应证书的 DNS SAN。

```toml
role = "publish"
bind = "0.0.0.0:0"
rendezvous = "192.0.2.1:7443"
rendezvous_name = "intro.internal.example"
peer_name = "publisher.internal.example"
peer_sha256 = "CONSUMER_SHA256"
local = "127.0.0.1:8080"
[tls]
cert = "publisher.pem"
key = "publisher.key"
ca = "ca.pem"
```

在消费端，将 `role` 改为 `"consume"`，`local` 改为本机空闲地址，例如 `"127.0.0.1:18080"`。将 `peer_sha256` 换成发布端指纹，`cert` 和 `key` 换成消费端自己的文件，`peer_name` 仍填发布端证书名称。

Linux 可使用 `conf/astrave-peer@.service` 常驻运行。创建系统用户和组 `astrave`，把二进制安装到 `/usr/local/bin/astrave`，把单元安装到 `/etc/systemd/system/`。端点配置放在 `/etc/astrave/peer/config.toml`，会合端配置放在 `/etc/astrave/rendezvous/config.toml`。让服务组可读取本机私钥，并限制其他用户访问。执行 `systemctl daemon-reload` 后，按角色执行 `systemctl enable --now astrave-peer@peer` 或 `systemctl enable --now astrave-peer@rendezvous`。可用 `journalctl -u astrave-peer@peer` 查看建连和重试记录。

当前每个会合实例配对两个预先登记的端点。禁用 UDP、按目的地址重新分配外部端口的 NAT，以及不支持内网回流的路由器都可能导致打洞失败。直连失败后端点会重试。有公网端口映射需求时，可另行部署现有网关配置。

### 验证连接

在 Ubuntu 24.04 上，仓库提供的 systemd 模板通过了 `systemd-analyze verify`。会合服务和发布端以 `astrave` 系统用户运行时，消费端成功访问了回环 HTTP 服务。停止会合服务后再次访问仍然成功。恢复会合服务并重启发布端后，消费端自动重连。

在消费端使用原服务的客户端访问本机 `local`。例如，HTTP 服务提供 `/health` 接口且消费端监听 18080 时，可执行 `curl --fail http://127.0.0.1:18080/health`。服务自身还使用 TLS 时，主机名也要匹配应用证书。停止会合服务后再发起一次请求，可以验证流量直连；测试断线恢复前应先恢复会合服务。

Windows 回环测试使用了两个真实服务。以消费端端口 18080 为例，Magpie 0.1.4465 可用 `magpie healthcheck --addr 127.0.0.1:18080` 检查。对处于初始设置阶段的 sub2api 0.2.24，可运行 `curl --fail http://127.0.0.1:18080/setup/status`，核对 `data.needs_setup` 为 `true`、`data.step` 为 `welcome`。两个服务均经 Astrave 完成检查，停止会合服务后重新发起请求也成功。这两项检查覆盖网关可达性。调用上游模型还需要配置服务商账户。

从源码目录运行 `cargo build --locked --bin astrave`，再执行 `python scripts/check-peer.py target/debug/astrave`。Windows 使用 `astrave.exe`，可通过 `--openssl PATH` 选择 OpenSSL。脚本为三个独立进程签发临时证书，并在本机模拟两个端口受限的 NAT。它验证服务端先发消息、TCP 半关闭、停止会合服务后并发回传八份 256 KiB 数据、发布端重启后消费端自动重连，以及证书指纹不匹配时拒绝访问。该检查验证所模拟的 NAT 规则下的功能，公网成功率需在实际网络测量。
