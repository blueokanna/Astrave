# 第三方内网服务接入

| 平台 | Astrave 入口 | 服务地址由谁配置 |
| --- | --- | --- |
| ngrok | `astrave provider ngrok web -c ngrok.yml` | ngrok 配置文件中名为 `web` 的 endpoint 指向本地服务 |
| NATAPP | `astrave provider natapp -c config.ini` | NATAPP 控制台配置隧道，客户端从 INI 文件读取凭据 |
| Sunny-Ngrok | `astrave provider sunny` | 新版客户端从 `SUNNY_TOKEN` 取得设备授权令牌，在控制台把隧道分配给上线的设备 |
| ZeroTier | `astrave p2p` 或常规 Astrave 网关配置 | ZeroTier 为已授权的网络成员分配 IP，Astrave 使用该 IP 建立连接 |
| Earthworm | 已有的固定 TCP 映射可以承载 Astrave TCP 网关连接 | 部署方提供通向指定网关的透明 TCP 字节流 |

ngrok、NATAPP 和 Sunny-Ngrok 需要在发布主机上安装相应的客户端。Astrave 在前台启动该程序并保留其输出。在 `PATH` 找不到程序时，使用 `--agent` 指定可执行文件。`-c` 传入厂商自己的配置文件。Sunny 新版客户端通过 `-t` 接收令牌，本机有权查看进程参数的账户可能读取它。[ngrok CLI](https://ngrok.com/docs/agent/cli#ngrok-start) · [NATAPP 配置](https://natapp.cn/article/config_ini) · [Sunny 客户端](https://ngrok.cc/docs/start/linux.html)

使用 ZeroTier 时，先让两台设备加入同一网络并完成授权。两份 libp2p 配置的 `addresses` 都填写发布端的 ZeroTier IP 和同一传输端口，`local` 填写各自的回环端口。使用常规网关时，服务端 `listen` 和客户端 `server` 使用网关的 ZeroTier IP，TLS 校验名须与证书一致。[ZeroTier 入门](https://docs.zerotier.com/start/) · [libp2p 双端配置](transport/libp2p.md)

使用已有固定 TCP 映射时，客户端 `server` 填写映射入口，`transport.protocol` 使用 `tcp`，保留网关认证和端到端 TLS 校验。Earthworm 的进程和路由由部署方管理。与实际 Earthworm 实例的互通尚待验证。

Cloudflare 命名隧道在 Astrave 进程内运行，公共域名及回源服务由 Cloudflare 控制台管理。[Cloudflare Tunnel](cloudflare.md)
