# Access a TCP service over libp2p

The publisher connects to one loopback service. The consumer gives local applications a loopback port for that service. The consumer needs a reachable publisher address, such as public IPv6, a mapped port or a ZeroTier address. For two hosts behind NAT, Astrave also provides a QUIC rendezvous mode. [QUIC hole punching](peer.md)

On each host run `astrave p2p-keygen identity.key`. This creates a new Ed25519 private key and prints the local Peer ID. An existing file causes the command to fail. Exchange the two IDs through a trusted channel. Keep each key on its host and restrict read access to the Astrave account.

Place each configuration alongside its host’s key. Replace `peerId` with the **other endpoint’s** full ID. Replace `192.0.2.10` with the publisher’s reachable IP address.

On the publisher, `publish.toml` forwards incoming connections to port 8080.

```toml
role = "publish"
identityFile = "identity.key"
peerId = "<consumer Peer ID>"
local = "127.0.0.1:8080"
addresses = ["/ip4/0.0.0.0/udp/4400/quic-v1", "/ip4/0.0.0.0/tcp/4400"]
```

On the consumer, `consume.toml` exposes the service on port 18080.

```toml
role = "consume"
identityFile = "identity.key"
peerId = "<publisher Peer ID>"
local = "127.0.0.1:18080"
addresses = ["/ip4/192.0.2.10/udp/4400/quic-v1", "/ip4/192.0.2.10/tcp/4400"]
```

Run `astrave p2p -c publish.toml` and `astrave p2p -c consume.toml` on the respective hosts. For an HTTP origin, run `curl http://127.0.0.1:18080/` on the consumer. Allow the configured ports through the publisher’s firewall. Use `/ip6/address/…` for IPv6.

The consumer tries up to eight addresses in order, with a ten-second handshake deadline per attempt. After exhausting the list it backs off from one to fifteen seconds between rounds. Applications can open new connections after reconnection and must reestablish interrupted TCP sessions. TCP uses Noise and yamux. QUIC v1 uses TLS. Both peers verify the remote Peer ID before negotiating `/astrave/tcp/1.0.0`. Each pair forwards up to 128 concurrent streams with a 16 KiB application copy buffer in each direction per stream.

Run `python scripts/check-libp2p.py <astrave-binary>` to exercise both transports, pin rejection, address fallback and reconnection. To measure eight concurrent loopback echoes and both processes’ resident memory, build the release binary and run `uv run --with psutil scripts/check-libp2p.py <astrave-binary> --measure`.

Each echo opens a new stream and verifies a 256 KiB payload. Throughput counts verified return bytes once. The script records both processes’ resident memory after warmup and samples it every 50 ms during five seconds of load, reporting the largest observation for each endpoint.
