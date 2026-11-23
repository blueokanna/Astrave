# 云网关与 DNS 证书

## 已验证的协议组合

| 客户端连接路径 | 发布的服务 | 发布版验证范围 |
| --- | --- | --- |
| TCP，多路复用、Token、mTLS | TCP、UDP、HTTP、HTTPS、SOCKS5 | TCP 与 UDP 回显，HTTP 域名和路径路由，HTTPS 透传及客户端 TLS 终止，SOCKS5 用户名密码认证及 CONNECT。随后热重载 HTTP 路由、重启网关并恢复 TCP 回显。 |
| WS 或 WSS | TCP | WS 直连回显；WSS 通过带 Bearer 鉴权的 Caddy，分别启用和关闭 `tcpMux`。外层验证入口证书，隧道内验证源站并提交客户端证书。 |
| QUIC 或 KCP | TCP | 开启 mTLS，两个方向各丢弃每第 20 个数据报，逐字节校验 256 KiB 回显。 |
| 点对点 QUIC | TCP | 两个模拟端口受限 NAT 后的节点完成直连。关闭会合服务后通过服务端先发、半关闭与八路并发回显；发布端重启可重连，错误证书指纹遭拒。 |

表中每行对应实际运行过的组合。复测入口为 `scripts/check-tunnels.py`（可传 `--caddy PATH`）和 `scripts/check-peer.py`。

## 选择网络入口

| 部署资源 | 配置 | 能承载的流量 |
| --- | --- | --- |
| 有公网地址的云主机 | 运行 `astrave-server`，把客户端 `server` 指向它。按实际用途在防火墙和安全组开放监听端口和业务端口。 | TCP、WS 使用 TCP 监听端口。QUIC、KCP 各需要一个可达的 UDP 端口。 |
| 四层负载均衡 | 同协议转发至网关。一个客户端的控制连接和数据连接需要命中持有该会话的同一网关。 | TCP 转发可承载 TCP 或 WS，UDP 转发需支持对应的双向会话。 |
| 支持 WebSocket 的 HTTPS 反向代理 | 入口终止外层 TLS，在配置的 `wsPath` 将 HTTP/1.1 Upgrade 转发至同一网关。客户端设置 `transport.protocol = "wss"`，并独立验证源站证书。 | 客户端到网关的双向字节流。访问者连接的 TCP/UDP 端口、HTTP/HTTPS 域名需要单独配置入口。 |

云主机路径可用于 AWS EC2、腾讯云 CVM、阿里云 ECS、Google Compute Engine、Azure VM、IBM Cloud 虚拟服务器、华为云 ECS 或提供等价网络能力的第三方。云厂商、地区和产品的长连接时限、UDP 转发能力与端口规则各不相同。本地通过了 Caddy 2.11.9 的 WSS 双向转发；`cloudflared 2026.11.1` 通过了示例 ingress 规则校验。逐云公网数据面仍待验证。[Cloudflare Tunnel 三端配置](transport/protocol/websocket.md)

对于需要 Bearer 令牌的 WSS 入口，在客户端增加以下配置，将完整头值（含 `Bearer ` 前缀）放入启动进程的环境变量。每次 Upgrade 都从该进程环境取值。入口完成鉴权后可在转发到源站前移除此头。

```toml
[transport.wsHeaderEnv]
Authorization = "ASTRAVE_EDGE_AUTH"
```

也可使用 `Api-Key`、`Ocp-Apim-Subscription-Key` 或 `X-` 开头的头名。入口须支持标准 `101 Switching Protocols` 和透明二进制数据。只提供 HTTP 请求响应、按消息执行函数或自定义消息封装的产品，需要额外的协议适配。

## 通过配置签发和续期证书

在管理机安装 lego，复制仓库中的 `conf/cloud-cert.toml`，填写 `provider`、`email`、`domains` 和 `storageDir`。`storageDir` 相对于配置文件目录，存放 ACME 账户、证书和私钥。CLI 调用本机 lego 的 DNS 驱动，凭据由驱动从环境变量或云身份读取。Flutter 证书页内置 Cloudflare、阿里云和腾讯云适配器。[lego DNS 驱动目录](https://go-acme.github.io/lego/dns/)

| 权威 DNS 服务 | `provider` | 驱动凭据示例 |
| --- | --- | --- |
| Cloudflare DNS | `cloudflare` | `CF_DNS_API_TOKEN`，需具备对应区域的读取及 DNS 编辑权限 |
| 腾讯云 DNSPod | `tencentcloud` | `TENCENTCLOUD_SECRET_ID`、`TENCENTCLOUD_SECRET_KEY` |
| 阿里云 DNS | `alidns` | 实例 RAM 角色或 `ALICLOUD_ACCESS_KEY`、`ALICLOUD_SECRET_KEY` |
| AWS Route 53 | `route53` | AWS 凭据链，如带托管角色的实例 |
| Google Cloud DNS | `gcloud` | Application Default Credentials，必要时指定 `GCE_PROJECT` |
| Microsoft Azure DNS | `azuredns` | Azure 身份及对应订阅、资源组，详见 `provider` 命令 |
| IBM Cloud Classic Infrastructure DNS | `ibmcloud` | `SOFTLAYER_USERNAME`、`SOFTLAYER_API_KEY`，适用于 SoftLayer |
| 华为云 DNS | `huaweicloud` | `HUAWEICLOUD_ACCESS_KEY_ID`、`HUAWEICLOUD_SECRET_ACCESS_KEY`、`HUAWEICLOUD_REGION` |
| 第三方 DNS API | `httpreq` | `HTTPREQ_ENDPOINT` 指向自有适配器。默认模式下 `/present` 和 `/cleanup` 接收含 `fqdn`、`value` 的 JSON。 |

```sh
astrave cloud-cert provider -c conf/cloud-cert.toml
astrave cloud-cert issue -c conf/cloud-cert.toml
astrave cloud-cert renew -c conf/cloud-cert.toml
```

`provider` 打印所装版本的凭据和可选参数。首次签发前阅读 CA 条款并设置 `acceptTerms = true`。示例的 `staging = true` 用于 Let's Encrypt 测试环境；生产签发时设置 `staging = false` 并使用独立的 `storageDir`。自建 ACME 服务可指定 HTTPS `directory`，私有 DNS 可指定 `dnsResolvers = ["127.0.0.1:53"]`。可用 `--lego PATH` 指定程序路径。

对示例主机名，网关可通过 `transport.tls.certFile` 和 `keyFile` 加载 `storageDir/certificates/gateway.example.com.crt` 和对应 `.key` 的绝对路径。部署续期证书后重启使用它的网关进程。上述八个云驱动都已通过 Astrave CLI 及 lego 4.26.0 的驱动查询。各云账号的权限和实际 TXT 写入需要在对应权威区验证。

本地端到端验证使用 Pebble 2.10.3 作为 ACME CA，`httpreq` 驱动连接回环 DNS 适配器。Astrave 的 `issue` 与 `renew` 均取得了匹配 `gateway.test` 的证书，续期后的证书内容发生变化。两次授权均查询到对应 TXT，随后清理了挑战记录。lego 的定时续期默认包含随机等待，命令可能在更新证书前停留数分钟。
