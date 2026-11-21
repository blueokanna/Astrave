# Direct connections between two services

`astrave rendezvous` introduces two peers by exchanging the UDP addresses it observes. Both peers contact it over mutual TLS from the same UDP socket they use for the subsequent QUIC connection. The publisher sends probes while the consumer initiates the encrypted handshake. Once connected, the consumer’s loopback TCP port forwards to exactly one loopback service on the publisher. The rendezvous can go offline without interrupting that connection.

Use three separate certificates signed by your private CA. The rendezvous certificate needs `serverAuth` and a DNS SAN matching `rendezvous_name`. The publisher needs `clientAuth`, `serverAuth` and a SAN matching `peer_name`. The consumer needs `clientAuth`. Determine each peer's leaf fingerprint with `openssl x509 -in peer.pem -noout -fingerprint -sha256` and remove the colons. Keep the CA signing key off all three hosts.

On the reachable host, allow the configured UDP port and run `astrave rendezvous -c config.toml` with this configuration. Replace the quoted fingerprint placeholders with the actual 64 hexadecimal digits.

```toml
bind = "0.0.0.0:7443"
peer_sha256 = ["PUBLISHER_SHA256", "CONSUMER_SHA256"]
[tls]
cert = "rendezvous.pem"
key = "rendezvous.key"
ca = "ca.pem"
```

Run `astrave peer -c config.toml` on each endpoint. Set `rendezvous` to the reachable UDP address. On the publisher, set `local` to the actual TCP listener used by Magpie, sub2api or another service. Its application-level authentication still applies.

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

On the consumer, use `role = "consume"`, the publisher’s fingerprint, the consumer’s certificate and key, and an unused loopback TCP port for `local`. Keep `peer_name` set to the publisher’s DNS SAN. Point your client at the consumer’s local port. Each QUIC stream carries one full-duplex TCP connection, including services that send a greeting before the client writes.

For systemd, create a system user and group named `astrave`, place the executable in `/usr/local/bin` and install `conf/astrave-peer@.service` in `/etc/systemd/system/`. Put the configuration and TLS files in `/etc/astrave/peer/` on endpoints or `/etc/astrave/rendezvous/` on the introducer. Grant the service group read access to its private key and restrict directory access to root and that group. Run `systemctl daemon-reload` and `systemctl enable --now astrave-peer@peer` or `astrave-peer@rendezvous`. `journalctl -u astrave-peer@peer` reports connection and retry events.

The introducer pairs two configured fingerprints. Direct connections require working UDP and a reachable peer mapping. Destination-dependent port mappings, networks blocking UDP and routers without NAT hairpin support can prevent a direct link. In that case the peers retry. The existing gateway profile provides a separately configurable forwarding path.

### Verify the connection

On Ubuntu 24.04, the packaged systemd template passed `systemd-analyze verify`. With the publisher and rendezvous running as the `astrave` service user, the consumer reached a loopback HTTP service, opened a fresh request while the rendezvous was stopped, and reconnected after the rendezvous returned and the publisher restarted.

On the consumer, use the service’s normal client against `local`. For example, for an HTTP service with a `/health` route, run `curl --fail http://127.0.0.1:18080/health` after configuring port 18080. TLS applications must still use a hostname covered by their application certificate. You can stop the rendezvous and open a second connection to confirm that the data travels directly. Restart it before testing reconnection.

Two Windows loopback checks used the actual services. With the consumer listening on port 18080, run `magpie healthcheck --addr 127.0.0.1:18080` for Magpie 0.1.4465. For a fresh sub2api 0.2.24 instance in setup mode, run `curl --fail http://127.0.0.1:18080/setup/status` and check that `data.needs_setup` is `true` and `data.step` is `welcome`. Both checks passed through Astrave, including a fresh request after the rendezvous stopped. These checks cover gateway reachability. Upstream model inference requires a configured provider account.

From a source checkout, `cargo build --locked --bin astrave` followed by `python scripts/check-peer.py target/debug/astrave` runs three processes with a disposable CA. Use `astrave.exe` on Windows and `--openssl PATH` to select OpenSSL. The test models two independent UDP mappings with address-and-port-dependent filters, checks server-first messages and TCP half-close, transfers eight simultaneous 256 KiB echoes after stopping the rendezvous, restarts the publisher and checks rejection of an incorrect leaf fingerprint. The simulator runs on loopback. Its results measure functional behavior under the modeled NAT filtering rules.
