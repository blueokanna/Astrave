# Cloud gateways and DNS certificates

## Verified protocol combinations

| Agent connection | Published service | Release test |
| --- | --- | --- |
| TCP with multiplexing, token and mTLS | TCP, UDP, HTTP, HTTPS, SOCKS5 | TCP and UDP echo, HTTP host/path routing, HTTPS passthrough and client-side termination, SOCKS5 username/password and CONNECT. Reloaded an HTTP route, restarted the gateway and recovered TCP echo. |
| WS or WSS | TCP | Direct WS echo and WSS through a Caddy proxy requiring bearer authentication. Both WSS multiplex modes carried data with separate edge TLS and origin mTLS. |
| QUIC or KCP | TCP | Verified a 256 KiB echo over mTLS while dropping every twentieth datagram in both directions. |
| Direct peer QUIC | TCP | Peers behind two simulated port-restricted NATs handled server-first data, half-close and eight concurrent echoes after rendezvous shutdown. Publisher restart recovered and the wrong certificate pin was rejected. |

Each row describes an exercised combination. Reproduce them with `scripts/check-tunnels.py` (optionally `--caddy PATH`) and `scripts/check-peer.py`.

## Choose a network entry point

| Resource | Configuration | Traffic |
| --- | --- | --- |
| Public VM | Run `astrave-server`, point the client `server` to it and allow the selected listener and service ports in host and cloud firewalls. | TCP and WS share a TCP listener. QUIC and KCP each need a reachable UDP port. |
| Layer 4 load balancer | Forward the matching transport. Route a client's control and data connections to the gateway holding that session. | TCP forwarding carries TCP or WS. UDP forwarding needs bidirectional session support. |
| HTTPS reverse proxy with WebSocket support | Terminate outer TLS and forward the HTTP/1.1 Upgrade on `wsPath` to the gateway. Set the client transport to `wss` and configure trust for the Astrave origin. | A bidirectional agent-to-gateway byte stream. Published service ports and virtual hosts need their own ingress. |

VM deployment applies to AWS EC2, Tencent CVM, Alibaba ECS, Google Compute Engine, Azure VMs, IBM Cloud virtual servers, Huawei ECS and comparable third-party hosts. Check listener support, idle limits and port rules for the actual cloud product and region. Local tests forwarded WSS through Caddy 2.11.9. `cloudflared 2026.11.1` validated the example ingress rules. Public cloud paths still require per-deployment tests. [Cloudflare Tunnel configuration](transport/protocol/websocket.md)

For an edge requiring a bearer token, place the entire header value including `Bearer ` in the client process environment and configure the variable name below. The client reads it on each WSS Upgrade. The proxy can remove the header after authentication and before forwarding to the origin.

```toml
[transport.wsHeaderEnv]
Authorization = "ASTRAVE_EDGE_AUTH"
```

`Api-Key`, `Ocp-Apim-Subscription-Key` and `X-` header names are also accepted. The edge must support a standard `101 Switching Protocols` response and transparent binary traffic. Products that invoke a function per message or impose their own message framing require an adapter.

## Issue and renew DNS certificates

Install lego on the management host and copy `conf/cloud-cert.toml` from the repository. Set `provider`, `email`, `domains` and `storageDir`. The storage directory is relative to the configuration file and contains the ACME account, certificates and private keys. The CLI invokes the installed lego DNS driver with the process environment and workload identity. The Flutter certificate page has native adapters for Cloudflare, Alibaba and Tencent. [lego DNS provider directory](https://go-acme.github.io/lego/dns/)

| Authoritative DNS product | `provider` | Credential examples |
| --- | --- | --- |
| Cloudflare DNS | `cloudflare` | `CF_DNS_API_TOKEN` with zone read and DNS edit for the target zone |
| Tencent Cloud DNSPod | `tencentcloud` | `TENCENTCLOUD_SECRET_ID`, `TENCENTCLOUD_SECRET_KEY` |
| Alibaba Cloud DNS | `alidns` | Instance RAM role or `ALICLOUD_ACCESS_KEY` and `ALICLOUD_SECRET_KEY` |
| Amazon Route 53 | `route53` | AWS credential chain, including instance roles |
| Google Cloud DNS | `gcloud` | Application Default Credentials, with `GCE_PROJECT` when needed |
| Microsoft Azure DNS | `azuredns` | Azure credentials and target subscription/resource group as listed by `provider` |
| IBM Cloud Classic Infrastructure DNS | `ibmcloud` | SoftLayer `SOFTLAYER_USERNAME` and `SOFTLAYER_API_KEY` |
| Huawei Cloud DNS | `huaweicloud` | `HUAWEICLOUD_ACCESS_KEY_ID`, `HUAWEICLOUD_SECRET_ACCESS_KEY`, `HUAWEICLOUD_REGION` |
| Third-party DNS API | `httpreq` | Set `HTTPREQ_ENDPOINT` to your adapter. In default mode its `/present` and `/cleanup` routes receive JSON with `fqdn` and `value`. |

```sh
astrave cloud-cert provider -c conf/cloud-cert.toml
astrave cloud-cert issue -c conf/cloud-cert.toml
astrave cloud-cert renew -c conf/cloud-cert.toml
```

`provider` displays the credentials and options for the installed driver. Review the CA subscriber agreement before setting `acceptTerms = true`. The sample selects Let's Encrypt staging. For production set `staging = false` and choose a separate `storageDir`. For a private CA, set an HTTPS `directory`. For private DNS, set `dnsResolvers` to an array of socket addresses. Pass `--lego PATH` to select the executable.

For the example hostname, set gateway `transport.tls.certFile` and `keyFile` to the absolute paths of `storageDir/certificates/gateway.example.com.crt` and its `.key`. Restart the gateway after deploying a renewed certificate. All eight cloud driver codes were checked through the Astrave CLI with lego 4.26.0. Account permissions and live TXT updates need verification against each authoritative zone.

In the local end-to-end test, Pebble 2.10.3 acted as the ACME CA and `httpreq` connected to a loopback DNS adapter. Both `issue` and `renew` produced a certificate matching `gateway.test`, with different certificate contents after renewal. Each authorization involved a TXT lookup followed by cleanup. lego applies a random scheduling delay during renewal by default, which can take several minutes.
