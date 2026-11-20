<img src="docs/static/img/logo.png" alt="Astrave" width="72" height="72">

# Astrave

[简体中文](README-zh.md)

Astrave publishes local services through a server you control. The Rust client opens an outbound connection to the server. Incoming connections to a configured port or virtual host travel back through that connection to the local service. The Flutter desktop app runs the same client as the command line.

![Astrave Desktop showing a connected local HTTP tunnel on Windows](docs/static/img/desktop.png)

## Try a tunnel on one machine

You need Rust 1.95.0 and three terminals in the project root. The example publishes a local HTTP service on port `8000` through port `9000`. Both the control connection and the published port bind to loopback. In the first terminal, start the Rust documentation preview on port `8000`.

```sh
cargo run --locked -p astrave-docs -- --serve
```

Start the server in the second terminal and the client in the third.

```sh
cargo run --locked -p astrave-server -- -c conf/astrave-server.toml
```

```sh
cargo run --locked -p astrave-client -- -c conf/astrave.toml
```

Open `http://127.0.0.1:9000/img/logo.png`. The browser receives the image from port `8000` through the tunnel. To try the desktop client, stop the CLI client, open `conf/astrave.toml` under **Profile**, then select **Connect** in **Overview**.

## Desktop

Use Flutter 3.47.9, the platform's Flutter desktop build tools and the pinned Rust toolchain. From `app/`, run the following on Windows. Use `linux` or `macos` as the build target on those hosts.

```sh
flutter pub get --enforce-lockfile
flutter build windows --release
```

On Windows, launch `app/build/windows/x64/runner/Release/astrave_desktop.exe` on x64 systems. Distribute the entire `Release` directory, including the Rust library, Flutter engine and `data/` assets.

| View | What you can do |
| --- | --- |
| Overview | Connect or disconnect, add or edit a tunnel, and copy its assigned endpoint. |
| Profile | Edit TOML directly, preserve comments, validate changes, save and reload the running client. |
| Activity | Search or clear the most recent 500 session log lines. |
| Certificates | Manage DNS credentials for Cloudflare, Alibaba Cloud or Tencent Cloud and request a Let’s Encrypt certificate through DNS-01. |

The **More** menu shows process CPU usage, resident memory and session uptime under **About Astrave**. On desktops with a system tray, use **Hide to tray** to keep the client running and the Astrave tray menu to reopen it. Language and theme choices are saved locally.

## Deployment

The loopback example uses plaintext TCP. For a remote deployment, configure the same `auth.token` on both peers, set `transport.tls.force = true` on the server, and supply its `certFile` and `keyFile`. Enable `transport.tls.enable` on the client. The client verifies the server name and certificate chain using the host trust store by default. Set the client’s `transport.tls.trustedCaFile` for a private CA. Setting that field on the **server** enables client-certificate authentication and requires a corresponding client certificate and key. Choose `listen` and `proxyAddr` explicitly for the interfaces you intend to publish on.

The Rust client supports TCP, UDP, HTTP, HTTPS and SOCKS5 tunnels with TCP, WebSocket, KCP or QUIC transports. See the full client and server examples for protocol-specific options, TLS and multiplexing. [Client configuration](conf/astrave-full.toml) · [Server configuration](conf/astrave-server-full.toml)

The server renders the browser dashboard in Rust. Set a loopback address, port, user and password under `[dashboard]`. Open that address to inspect clients, tunnels and traffic. Use HTTPS through a reverse proxy for remote administration.

```sh
cargo build --release --locked -p astrave-server -p astrave-client
```

## Source layout

The performance report includes the Windows measurements, time and space complexity and reproduction commands. [Performance report](docs/en/performance.md)

| Path | Responsibility |
| --- | --- |
| `proto/`, `core/` | Wire messages, codecs, authentication, transports and configuration |
| `client/`, `server/` | Rust agent, CLI, tunnel routing and dashboard API |
| `app/`, `desktop/` | Material 3 Expressive Flutter interface and Rust runtime exposed through `flutter_rust_bridge` |
| `server/src/dashboard/` | Server-rendered web dashboard and JSON API |
| `docs/` | English and Chinese Markdown, screenshots and Rust static site generator |

## License

Apache License 2.0. [License text](LICENSE)

The desktop app uses Google Sans and HarmonyOS Sans. Both font licenses are included in the app and available under **About Astrave → View licenses**. [Google Sans license](app/assets/fonts/GoogleSans-OFL.txt) · [HarmonyOS Sans license](app/assets/fonts/HarmonyOS-Sans-License.txt)
