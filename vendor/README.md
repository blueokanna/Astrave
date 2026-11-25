# Cloudflare connector dependencies

`libcfd` and `libcfd-rpc` are vendored from
https://github.com/apersomany/libcfd at commit
`fe57dd3679a02a57668581cfa2c89e6364ff7f2e` (0.3.0 development source).
They are linked into Astrave; no cloudflared executable is launched.
The Apache-2.0 license is preserved from the published libcfd 0.2.0 crate.

Astrave additions expose connection indices, registration state and a fallible
raw remote-configuration callback. The latter lets Astrave validate the entire
ingress configuration before acknowledging it to Cloudflare. Unsupported
configuration is rejected instead of silently acknowledging an incomplete route.
Registration advertises only implemented capabilities.
Automatic transport fallback counts individual failed edge connections. The
quick-tunnel test API races resolved addresses to avoid a blackholed first address.
Edge discovery prefers IPv4 before trying IPv6. HTTP request readers respect
Content-Length on QUIC, and WebSocket responders can forward an HTTP rejection.
Bidirectional stream copies poll each direction independently under backpressure.

The committed Cap'n Proto output permits builds without the capnp compiler.
Upstream offline tests are retained; live tests require explicit credentials.
