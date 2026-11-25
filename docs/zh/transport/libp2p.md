# 通过 libp2p 访问一个 TCP 服务

发布端连接本机回环服务，消费端向本机应用提供监听端口。消费端需要能访问发布端的 IP 和端口，例如通过公网 IPv6、路由器端口映射或同一个 ZeroTier 网络。两端都在 NAT 后且没有可达地址时，可使用带会合服务的 QUIC 打洞模式。[QUIC 打洞配置](peer.md)

在两台主机上分别运行 `astrave p2p-keygen identity.key`。命令创建 Ed25519 私钥，输出该端的 Peer ID。如果文件已存在，命令会报错。通过可信渠道交换输出的 ID，私钥留在各自的主机上，并限制为运行 Astrave 的账户可读。

将下列两份配置分别放在对应主机的私钥目录中。把 `peerId` 替换为**另一端**的完整 ID。将消费端配置中的 `192.0.2.10` 替换为实际可达 IP。

发布端 `publish.toml` 将连接转发到本机 8080 端口。

```toml
role = "publish"
identityFile = "identity.key"
peerId = "<消费端 Peer ID>"
local = "127.0.0.1:8080"
addresses = ["/ip4/0.0.0.0/udp/4400/quic-v1", "/ip4/0.0.0.0/tcp/4400"]
```

消费端 `consume.toml` 在本机 18080 端口提供同一服务。

```toml
role = "consume"
identityFile = "identity.key"
peerId = "<发布端 Peer ID>"
local = "127.0.0.1:18080"
addresses = ["/ip4/192.0.2.10/udp/4400/quic-v1", "/ip4/192.0.2.10/tcp/4400"]
```

分别运行 `astrave p2p -c publish.toml` 和 `astrave p2p -c consume.toml`。如果源站提供 HTTP，在消费端运行 `curl http://127.0.0.1:18080/` 即可验证。防火墙需允许所选传输的端口。IPv6 地址使用 `/ip6/地址/…`。

消费端按顺序尝试最多 8 个地址，每次握手上限为 10 秒。地址全部尝试后按 1 至 15 秒退避重试。重连后的新连接可以继续使用，已中断的 TCP 会话需由应用重新建立。TCP 使用 Noise 和 yamux，QUIC v1 使用 TLS。两端都先核对 Peer ID，再协商 `/astrave/tcp/1.0.0`。每对端点最多转发 128 路并发流，每路每个方向的应用复制缓冲区为 16 KiB。

运行 `python scripts/check-libp2p.py <astrave可执行文件>` 可验证两种传输、身份拒绝、地址回退和重连。使用 release 二进制及 `uv run --with psutil scripts/check-libp2p.py <astrave可执行文件> --measure` 可测量本机 8 路并发回显的有效负载吞吐和两个端点的驻留内存。

每次回显都会新建一条流，发送并校验 256 KiB 数据。吞吐量仅计已校验的返回字节。脚本在预热后记录两个进程的驻留内存，并在 5 秒负载期间每 50 ms 采样一次，分别输出发布端和消费端的最大采样值。
