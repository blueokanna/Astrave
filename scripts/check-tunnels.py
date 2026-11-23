"""Exercise the release binaries on loopback with an ephemeral CA and real sockets."""

import argparse
import asyncio
import base64
import http.client
import json
import os
import secrets
import select
import shutil
import socket
import socketserver
import ssl
import subprocess
import tempfile
import threading
import time
from contextlib import ExitStack, contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HOST = "127.0.0.1"
PAYLOAD = bytes(range(256)) * 1024


def ports(count, kind=socket.SOCK_STREAM):
    with ExitStack() as stack:
        sockets = [stack.enter_context(socket.socket(type=kind)) for _ in range(count)]
        for sock in sockets:
            sock.bind((HOST, 0))
        return [sock.getsockname()[1] for sock in sockets]


def certificates(work, openssl, identities=(("server", "serverAuth"), ("client", "clientAuth"))):
    def run(*arguments):
        subprocess.run([openssl, *arguments], cwd=work, check=True, capture_output=True)

    run(
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-days",
        "2",
        "-subj",
        "/CN=Astrave loopback test CA",
        "-addext",
        "basicConstraints=critical,CA:TRUE",
        "-addext",
        "keyUsage=critical,keyCertSign,cRLSign",
        "-keyout",
        "ca.key",
        "-out",
        "ca.pem",
    )
    for name, usage in identities:
        run(
            "req",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=" + name,
            "-keyout",
            name + ".key",
            "-out",
            name + ".csr",
        )
        ext = work / (name + ".ext")
        ext.write_text(
            f"extendedKeyUsage={usage}\nsubjectAltName=DNS:localhost,DNS:pass.test,DNS:term.test\n"
        )
        run(
            "x509",
            "-req",
            "-days",
            "2",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-CAcreateserial",
            "-in",
            name + ".csr",
            "-out",
            name + ".pem",
            "-extfile",
            ext.name,
        )


class HttpOrigin(BaseHTTPRequestHandler):
    def do_GET(self):
        body = f"{self.path} {self.headers['Host']}".encode()
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


class Echo(socketserver.BaseRequestHandler):
    def handle(self):
        try:
            while chunk := self.request.recv(65536):
                self.request.sendall(chunk)
        except ConnectionResetError:
            return


class UdpEcho(socketserver.BaseRequestHandler):
    def handle(self):
        data, sock = self.request
        sock.sendto(data, self.client_address)


def serve(stack, server):
    stack.enter_context(server)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    stack.callback(server.shutdown)
    return server.server_address[1]


class LossRelay:
    """One UDP association for a multiplexed KCP or QUIC session."""

    def __init__(self, stack, upstream):
        sock = stack.enter_context(socket.socket(type=socket.SOCK_DGRAM))
        sock.bind((HOST, 0))
        self.port = sock.getsockname()[1]
        self.seen = [0, 0]
        self.dropped = [0, 0]
        stop = threading.Event()

        def forward():
            peer = None
            while not stop.is_set():
                if not select.select([sock], [], [], 0.1)[0]:
                    continue
                try:
                    payload, source = sock.recvfrom(65535)
                except ConnectionResetError:
                    # Windows reports ICMP port-unreachable here if the first
                    # datagram arrives before the gateway has bound its port.
                    continue
                direction = int(source == (HOST, upstream))
                if direction == 0:
                    peer = source
                self.seen[direction] += 1
                if self.seen[direction] % 20 == 0:
                    self.dropped[direction] += 1
                elif peer is not None:
                    sock.sendto(payload, peer if direction else (HOST, upstream))

        thread = threading.Thread(target=forward, daemon=True)
        thread.start()
        stack.callback(thread.join)
        stack.callback(stop.set)


def receive(sock, count):
    buf = bytearray()
    while len(buf) < count:
        part = sock.recv(count - len(buf))
        if not part:
            raise EOFError("Tunnel closed during echo")
        buf.extend(part)
    return buf


@contextmanager
def running(bin_dir, work, exe, cfg, name, env):
    path = work / (name + ".toml")
    path.write_text(cfg)
    log = work / (name + ".log")
    with log.open("wb") as output:
        proc = subprocess.Popen(
            [bin_dir / exe, "-c", path],
            cwd=work,
            env=env,
            stdout=output,
            stderr=subprocess.STDOUT,
        )
        try:
            yield proc, path, log
        finally:
            if proc.poll() is None:
                proc.terminate()
            try:
                proc.wait(5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


def get_http(port, host, path, context=None, credentials=None):
    with ExitStack() as stack:
        sock = stack.enter_context(socket.create_connection((HOST, port), 5))
        if context:
            sock = stack.enter_context(context.wrap_socket(sock, server_hostname=host))
        auth = (
            ""
            if credentials is None
            else "Authorization: Basic " + credentials + "\r\n"
        )
        sock.sendall(
            f"GET {path} HTTP/1.1\r\nHost: {host}\r\n{auth}Connection: close\r\n\r\n".encode()
        )
        with http.client.HTTPResponse(sock) as res:
            res.begin()
            return res.status, res.read()


def ready(mapped, client, log):
    deadline = time.monotonic() + 35
    while time.monotonic() < deadline and client.poll() is None:
        try:
            return socket.create_connection((HOST, mapped), 1)
        except OSError:
            time.sleep(0.1)
    raise RuntimeError(
        "Registration failed\n" + log.read_text(errors="replace")[-3000:]
    )


def check(bin_dir, work, proto, origins, tls, *, wss_edge=None, mux=True):
    control, mapped, http, https, socks, dashboard = ports(6)
    if wss_edge:
        control, edge_port, access_id, access_secret = wss_edge
    mux_value = str(mux).lower()
    transport_udp, mapped_udp = ports(2, socket.SOCK_DGRAM)
    echo, udp, web, web_tls = origins
    token = secrets.token_hex(16)
    env = {
        **os.environ,
        "ASTRAVE_CONTROL_SOCKET": (
            rf"\\.\pipe\astrave-test-{token}"
            if os.name == "nt"
            else str(work / "ctl.sock")
        ),
    }
    if wss_edge:
        env["ASTRAVE_TEST_EDGE_AUTH"] = f"Bearer {access_secret}"
    with ExitStack() as stack:
        loss = LossRelay(stack, transport_udp) if proto in ("quic", "kcp") else None
        extra = f"{proto}Port={transport_udp}\n" if loss else ""
        server = (
            f'listen="{HOST}:{control}"\nproxyAddr="{HOST}"\n{extra}httpGwPort={http}\nhttpsGwPort={https}\n'
            f'auth.token="{token}"\ntransport.tcpMux={mux_value}\n[transport.tls]\nforce=true\ncertFile="server.pem"\n'
            f'keyFile="server.key"\ntrustedCaFile="ca.pem"\n[dashboard]\naddr="{HOST}"\nport={dashboard}\nuser="test"\npassword="{token}"\n'
        )
        if proto == "tcp":
            # Keep the accepted connection open after replacing the listening
            # socket. Recovery must come from the client's handshake deadline.
            stalled = stack.enter_context(socket.socket())
            stalled.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            stalled.bind((HOST, control))
            stalled.listen(1)
            stalled.settimeout(20)
        else:
            stack.enter_context(
                running(bin_dir, work, "astrave-server", server, "gateway", env)
            )
        endpoint = (
            f"localhost:{edge_port}" if wss_edge else f"{HOST}:{loss.port if loss else control}"
        )
        edge_options = (
            'transport.wsTrustedCaFile="edge/ca.pem"\n'
            'transport.wsHeaderEnv.Authorization="ASTRAVE_TEST_EDGE_AUTH"\n'
            f'transport.cloudflareAccess.clientId="{access_id}"\n'
            f'transport.cloudflareAccess.clientSecret="{access_secret}"\n'
            if wss_edge
            else ""
        )
        cfg = (
            f'server="{endpoint}"\nauth.token="{token}"\ntransport.protocol="{proto}"\n{edge_options}'
            f'transport.tcpMux={mux_value}\n[transport.tls]\nenable=true\nserverName="pass.test"\n'
            'trustedCaFile="ca.pem"\ncertFile="client.pem"\nkeyFile="client.key"\n'
            f'[[tunnels]]\nname="echo"\nprotocol="tcp"\nservice="{HOST}:{echo}"\nremotePort={mapped}\n'
        )
        if proto == "tcp":
            for name, kind, options in [
                ("udp", "udp", f'service="{HOST}:{udp}"\nremotePort={mapped_udp}'),
                (
                    "web",
                    "http",
                    f'service="{HOST}:{web}"\ndomains=["site.test"]\nlocations=["/api"]\nhostHeaderRewrite="origin.test"',
                ),
                ("pass", "https", f'service="{HOST}:{web_tls}"\ndomains=["pass.test"]'),
                (
                    "term",
                    "https",
                    f'domains=["term.test"]\n[tunnels.plugin]\ntype="tls-term"\nservice="{HOST}:{web}"\ncertFile="server.pem"\nkeyFile="server.key"',
                ),
                (
                    "socks",
                    "tcp",
                    f'remotePort={socks}\n[tunnels.plugin]\ntype="socks5"\nusername="u"\npassword="p"',
                ),
            ]:
                cfg += f'[[tunnels]]\nname="{name}"\nprotocol="{kind}"\n{options}\n'
        client, path, log = stack.enter_context(
            running(bin_dir, work, "astrave", cfg, "agent", env)
        )
        if proto == "tcp":
            held, _ = stalled.accept()
            stack.enter_context(held)
            stalled.close()
            gateway_process, _, _ = stack.enter_context(
                running(bin_dir, work, "astrave-server", server, "gateway", env)
            )
        with ready(mapped, client, log) as sock:
            sock.settimeout(30)
            sock.sendall(PAYLOAD)
            assert receive(sock, len(PAYLOAD)) == PAYLOAD
        if loss:
            assert min(loss.dropped) > 0
            print(
                proto,
                "mTLS echo",
                len(PAYLOAD),
                "bytes, datagrams seen",
                loss.seen,
                "dropped",
                loss.dropped,
                flush=True,
            )
        if proto != "tcp":
            return
        with socket.create_connection((HOST, control), 5) as raw:
            try:
                with tls.wrap_socket(raw, server_hostname="localhost") as anonymous:
                    anonymous.recv(1)
            except ssl.SSLError as err:
                if "CERTIFICATE_REQUIRED" not in str(err):
                    raise
            else:
                raise AssertionError("Missing client certificate was accepted")
        credentials = base64.b64encode(f"test:{token}".encode()).decode()
        deadline = time.monotonic() + 15
        while True:
            status, body = get_http(
                dashboard, HOST, "/api/v1/tunnels?pageSize=10", credentials=credentials
            )
            if status == 200 and json.loads(body)["data"]["total"] == 6:
                break
            if time.monotonic() > deadline:
                raise RuntimeError("Dashboard did not list all six tunnels")
            time.sleep(0.1)
        assert get_http(dashboard, HOST, "/api/v1/tunnels")[0] == 401
        status, body = get_http(
            dashboard, HOST, "/api/v1/system/info", credentials=credentials
        )
        assert status == 200
        usage = json.loads(body)["data"]["process"]
        assert usage["residentBytes"] > 0 and usage["cpuPercent"] >= 0
        for gateway, host, request, response, ctx in [
            (http, "site.test", "/api/x", b"/api/x origin.test", None),
            (https, "pass.test", "/pass", b"/pass pass.test", tls),
            (https, "term.test", "/term", b"/term term.test", tls),
        ]:
            assert get_http(gateway, host, request, ctx) == (200, response)
        assert get_http(http, "site.test", "/outside")[0] == 404
        with socket.socket(type=socket.SOCK_DGRAM) as sock:
            sock.settimeout(5)
            sock.sendto(b"Astrave UDP", (HOST, mapped_udp))
            assert sock.recv(1024) == b"Astrave UDP"
        with socket.create_connection((HOST, socks), 5) as sock:
            sock.sendall(b"\x05\x01\x02")
            assert receive(sock, 2) == b"\x05\x02"
            sock.sendall(b"\x01\x01u\x01p")
            assert receive(sock, 2) == b"\x01\x00"
            sock.sendall(
                b"\x05\x01\x00\x01" + socket.inet_aton(HOST) + echo.to_bytes(2, "big")
            )
            assert receive(sock, 4) == b"\x05\x00\x00\x01"
            receive(sock, 6)
            sock.sendall(b"Astrave SOCKS5")
            assert receive(sock, 14) == b"Astrave SOCKS5"
        path.write_text(cfg.replace('locations=["/api"]', 'locations=["/updated"]'))
        subprocess.run(
            [bin_dir / "astrave", "reload", "-c", path],
            env=env,
            cwd=work,
            check=True,
            timeout=15,
            capture_output=True,
        )
        deadline = time.monotonic() + 10
        while get_http(http, "site.test", "/updated")[0] != 200:
            if time.monotonic() > deadline:
                raise RuntimeError("HTTP route reload failed")
            time.sleep(0.1)
        assert get_http(http, "site.test", "/api")[0] == 404
        gateway_process.kill()
        gateway_process.wait(5)
        stack.enter_context(
            running(bin_dir, work, "astrave-server", server, "restart", env)
        )
        with ready(mapped, client, log) as sock:
            sock.settimeout(10)
            sock.sendall(PAYLOAD)
            assert receive(sock, len(PAYLOAD)) == PAYLOAD
        print(
            "Recovered after a stalled handshake and gateway restart with the same client process",
            flush=True,
        )
        print(
            "TCP multiplexing, token and mTLS, UDP, HTTP, both HTTPS modes, SOCKS5 CONNECT, dashboard authentication and tunnel reload passed",
            flush=True,
        )


async def check_wss(bin_dir, work, origins, tls, edge_tls, mux):
    (control,) = ports(1)
    access_id, access_secret = secrets.token_hex(16), secrets.token_hex(32)
    edge_names = []
    edge_tls.set_servername_callback(lambda sock, name, ctx: edge_names.append(name))
    client_certs = []

    async def proxy(reader, writer):
        # Terminate only the public TLS layer. The gateway receives HTTP Upgrade
        # and authenticates the Astrave client inside the upgraded stream.
        client_certs.append(writer.get_extra_info("peercert"))
        upstream = None
        tasks = []
        try:
            head = await reader.readuntil(b"\r\n\r\n")
            fields = head.split(b"\r\n")
            if (
                f"CF-Access-Client-Id: {access_id}".encode() not in fields
                or f"CF-Access-Client-Secret: {access_secret}".encode() not in fields
                or f"Authorization: Bearer {access_secret}".encode() not in fields
            ):
                writer.write(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                await writer.drain()
                return
            source, upstream = await asyncio.open_connection(HOST, control)
            upstream.write(head)
            await upstream.drain()

            async def copy(src, dst):
                while data := await src.read(65536):
                    dst.write(data)
                    await dst.drain()

            tasks = [
                asyncio.create_task(copy(reader, upstream)),
                asyncio.create_task(copy(source, writer)),
            ]
            await asyncio.wait(tasks, return_when=asyncio.FIRST_COMPLETED)
        except (OSError, asyncio.IncompleteReadError):
            pass
        finally:
            for task in tasks:
                task.cancel()
            await asyncio.gather(*tasks, return_exceptions=True)
            writer.close()
            if upstream:
                upstream.close()

    async with await asyncio.start_server(proxy, HOST, 0, ssl=edge_tls) as edge:
        port = edge.sockets[0].getsockname()[1]
        await asyncio.to_thread(
            check, bin_dir, work, "wss", origins, tls,
            wss_edge=(control, port, access_id, access_secret), mux=mux,
        )
    assert edge_names and all(name == "localhost" for name in edge_names)
    assert client_certs and all(cert is None for cert in client_certs)


def check_caddy(caddy, bin_dir, work, origins, tls):
    control, port = ports(2)
    secret = secrets.token_hex(32)
    config = work / "Caddyfile"
    config.write_text(f"""{{
    admin off
    auto_https off
    persist_config off
}}
https://localhost:{port} {{
    bind {HOST}
    tls edge/server.pem edge/server.key
    route {{
        @authorized header Authorization "Bearer {secret}"
        reverse_proxy @authorized {HOST}:{control} {{
            header_up -Authorization
            header_up -CF-Access-Client-Id
            header_up -CF-Access-Client-Secret
        }}
        respond 403
    }}
}}
""")
    with subprocess.Popen(
        [caddy, "run", "--config", config, "--adapter", "caddyfile"],
        cwd=work,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        env={
            **os.environ,
            "XDG_DATA_HOME": str(work),
            "XDG_CONFIG_HOME": str(work),
            "APPDATA": str(work),
            "LOCALAPPDATA": str(work),
        },
    ) as edge:
        try:
            for mux in (True, False):
                check(
                    bin_dir, work, "wss", origins, tls,
                    wss_edge=(control, port, "integration", secret), mux=mux,
                )
            trust = ssl.create_default_context(cafile=str(work / "edge" / "ca.pem"))
            assert get_http(port, "localhost", "/", trust)[0] == 403
            print(
                "Caddy WSS bearer authentication, origin mTLS and echo verified in both multiplex modes",
                flush=True,
            )
        finally:
            edge.terminate()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", type=Path)
    parser.add_argument("--openssl", default=shutil.which("openssl") or "openssl")
    parser.add_argument(
        "--caddy", help="also verify the WSS transport through this Caddy executable"
    )
    args = parser.parse_args()
    with (
        tempfile.TemporaryDirectory(prefix="astrave-integration-") as temp,
        ExitStack() as stack,
    ):
        work = Path(temp)
        certificates(work, args.openssl)
        tls = ssl.create_default_context(cafile=str(work / "ca.pem"))
        tls_server = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls_server.load_cert_chain(work / "server.pem", work / "server.key")
        web_tls = ThreadingHTTPServer((HOST, 0), HttpOrigin)
        web_tls.socket = tls_server.wrap_socket(web_tls.socket, server_side=True)
        origins = [
            serve(stack, s)
            for s in (
                socketserver.ThreadingTCPServer((HOST, 0), Echo),
                socketserver.UDPServer((HOST, 0), UdpEcho),
                ThreadingHTTPServer((HOST, 0), HttpOrigin),
                web_tls,
            )
        ]
        for proto in ["tcp", "websocket", "quic", "kcp"]:
            check(args.binaries.resolve(), work, proto, origins, tls)
            print(proto, "verified", flush=True)
        edge = work / "edge"
        edge.mkdir()
        certificates(edge, args.openssl, (("server", "serverAuth"),))
        edge_tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        edge_tls.load_cert_chain(edge / "server.pem", edge / "server.key")
        edge_tls.load_verify_locations(work / "ca.pem")
        edge_tls.verify_mode = ssl.CERT_OPTIONAL
        for mux in (True, False):
            asyncio.run(check_wss(args.binaries.resolve(), work, origins, tls, edge_tls, mux))
            print(f"WSS edge and origin mTLS verified (tcpMux={mux})", flush=True)
        if args.caddy:
            check_caddy(args.caddy, args.binaries.resolve(), work, origins, tls)


if __name__ == "__main__":
    main()
