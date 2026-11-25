# Third-party service publishing

| Provider | Astrave entry point | Service configuration |
| --- | --- | --- |
| ngrok | `astrave provider ngrok web -c ngrok.yml` | Point the ngrok endpoint named `web` at the local service in the agent config |
| NATAPP | `astrave provider natapp -c config.ini` | Configure the tunnel in the NATAPP dashboard and put agent credentials in the INI file |
| Sunny-Ngrok | `astrave provider sunny` | Set `SUNNY_TOKEN` to the device authorization token for the current agent and assign the tunnel to the connected device in the dashboard |
| ZeroTier | `astrave p2p` or a standard Astrave gateway profile | Use the managed IP assigned to an authorized network member |
| Earthworm | An existing fixed TCP mapping can carry an Astrave TCP gateway connection | The deployment must already provide a transparent byte stream to the designated Astrave gateway |

Install the ngrok, NATAPP or Sunny-Ngrok agent on the publishing host. Astrave runs the selected executable in the foreground and inherits its output. Supply `--agent` if the executable is outside `PATH`. The `-c` argument selects the vendor’s own configuration file. The current Sunny agent accepts its token as `-t`, which makes it visible to local accounts with access to the process arguments. [ngrok CLI](https://ngrok.com/docs/agent/cli#ngrok-start) · [NATAPP configuration](https://natapp.cn/article/config_ini) · [Sunny agent](https://ngrok.cc/docs/start/linux.html)

For ZeroTier, join and authorize both hosts on the same network. In both libp2p profiles, set `addresses` to the publisher’s managed IP and the chosen transport port. Keep `local` on loopback at each end. For an Astrave gateway, use the gateway’s managed IP in its `listen` address and the client’s `server` address. Match the TLS verification name to the gateway certificate. [ZeroTier setup](https://docs.zerotier.com/start/) · [libp2p profiles](transport/libp2p.md)

For an existing fixed TCP mapping, set the client’s `server` to the entry point and `transport.protocol` to `tcp`. Keep gateway authentication and end-to-end TLS verification enabled. The operator manages the Earthworm process and its routes. Interoperability with a running Earthworm instance remains unverified.

Cloudflare named tunnels run inside Astrave and use the public hostnames and origin services configured in the Cloudflare dashboard. [Cloudflare Tunnel](cloudflare.md)
