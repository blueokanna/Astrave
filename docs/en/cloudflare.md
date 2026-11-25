# Native Cloudflare Tunnel

Astrave embeds a Rust Cloudflare connector. Dashboard-managed named tunnels
use the installation page's Tunnel Token directly; cloudflared and astrave-server
are not required.

On the Profile page choose **Cloudflare Tunnel**, paste the connector token,
optionally enter its Tunnel ID, choose the transport, then review and save the
configuration. Connect from Overview. Configure published application routes in
the Cloudflare dashboard, for example `app.example.com` to `http://127.0.0.1:8080`.
Loopback addresses refer to the device running Astrave.

For the CLI, use `conf/cloudflare.toml`. Set `CLOUDFLARE_TUNNEL_TOKEN` in the process
environment or replace `token` with `tokenFile`, pointing to a UTF-8 file containing
only the token. Relative paths resolve against the configuration directory.
Use exactly one token source. The optional `tunnelId` is checked against the token.

```sh
astrave verify -c conf/cloudflare.toml
astrave -c conf/cloudflare.toml
```

This mode uses `[cloudflare]` and dashboard ingress rules, without `[[tunnels]]`.
Reloading local settings reconnects the connector. Remote configuration is applied
atomically after validation. Switch connection modes while disconnected.

Implemented: named registration, QUIC and HTTP/2 with fallback, up to four edge
connections, reconnect, ordered hostname/wildcard/path routing, streaming HTTP,
HTTPS, SSE, WebSocket upgrades, public TCP/SSH/RDP/SMB over WebSocket, Host rewriting,
origin TLS name/CA configuration and explicit noTLSVerify.
Origin dialing uses system DNS and staggers IPv4 and IPv6 attempts by 250 ms, with at most two pending sockets per request. Failed attempts release their slot immediately. `noHappyEyeballs` selects sequential dialing. The connector also supports TCP keepalive, connect/TLS timeouts and `matchSNItoHost`.

Not implemented: WARP private TCP/UDP/ICMP, HTTP/2/gRPC origins, HTTP trailers,
bastion/SOCKS/Unix sockets, origin Access JWT validation, the management logging
protocol, origin connection pooling/tuning (each request opens an origin connection),
and user-facing locally managed credentials/quick tunnel commands.
Unsupported remote options fail configuration application and retain the previous
valid configuration. This is not complete cloudflared feature parity.

Run offline connector tests with `cargo test -p astrave --features app --lib cloudflare`.
The ignored `live_native_cloudflare_http2_and_quic` test creates temporary public
quick tunnels serving only a fixed test response. Run it explicitly with
`-- --ignored --nocapture`. Neither offline nor quick tunnel tests establish
successful operation of a particular account's named tunnel; that requires its
credentials, routes and network environment.

On 2026-11-25 the Windows quick tunnel test registered over both HTTP/2 and QUIC. Public HTTPS requests returned the expected `astrave-native-edge-proof` response over each transport.
