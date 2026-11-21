"""Verify pinned mTLS, bidirectional streams and independence from the introducer."""

import argparse
from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack
import hashlib
from pathlib import Path
import runpy
import select
import shutil
import socket
import socketserver
import ssl
import subprocess
import tempfile
import threading
import time

network = runpy.run_path(str(Path(__file__).with_name("check-tunnels.py")))


class GreetingEcho(network["Echo"]):
    def handle(self):
        self.request.sendall(b"hello")
        super().handle()


class NatPair:
    """Two endpoint-independent UDP mappings with address-and-port-dependent filtering.

    Both private and public sockets live on loopback. A direct packet addressed to
    a public port is delivered from the other mapping only if the receiver has
    previously sent a packet there. The harness does not inspect QUIC contents.
    """

    def __init__(self, stack, broker, inside):
        def bind():
            sock = stack.enter_context(socket.socket(type=socket.SOCK_DGRAM))
            sock.bind(("127.0.0.1", 0))
            return sock

        self.intro = bind()
        self.public = [bind(), bind()]
        self.opened = [False, False]
        self.direct_packets = [0, 0]
        stopped = threading.Event()

        def route():
            while not stopped.is_set():
                for sock in select.select([self.intro, *self.public], [], [], 0.1)[0]:
                    try:
                        packet, sender = sock.recvfrom(65535)
                    except ConnectionResetError:
                        # Winsock reports ICMP port-unreachable while a peer restarts.
                        continue
                    if sock is self.intro and sender in inside:
                        self.public[inside.index(sender)].sendto(packet, broker)
                    elif sock in self.public:
                        dst = self.public.index(sock)
                        src = 1 - dst
                        if sender == broker:
                            self.intro.sendto(packet, inside[dst])
                        elif sender == inside[src]:
                            self.opened[src] = True
                            if self.opened[dst]:
                                self.public[src].sendto(packet, inside[dst])
                                self.direct_packets[src] += 1

        worker = threading.Thread(target=route, daemon=True)
        worker.start()
        stack.callback(worker.join)
        stack.callback(stopped.set)


def check(binary, openssl):
    with tempfile.TemporaryDirectory() as temp, ExitStack() as stack:
        work = Path(temp)
        network["certificates"](
            work,
            openssl,
            [
                ("intro", "serverAuth"),
                ("publisher", "serverAuth,clientAuth"),
                ("consumer", "clientAuth"),
            ],
        )

        def fingerprint(name):
            return hashlib.sha256(
                ssl.PEM_cert_to_DER_cert((work / f"{name}.pem").read_text())
            ).hexdigest()

        def start(name, command, conf):
            tls = f'[tls]\ncert="{name}.pem"\nkey="{name}.key"\nca="ca.pem"\n'
            path = work / f"{name}.toml"
            path.write_text(conf + tls)
            log = stack.enter_context((work / f"{name}.log").open("wb"))
            proc = subprocess.Popen(
                [binary, command, "-c", str(path)],
                cwd=work,
                stdout=log,
                stderr=subprocess.STDOUT,
            )

            def stop():
                if proc.poll() is None:
                    proc.terminate()
                proc.wait(timeout=5)

            stack.callback(stop)
            return proc

        broker_port, publisher_udp, consumer_udp = network["ports"](
            3, socket.SOCK_DGRAM
        )
        nat = NatPair(
            stack,
            ("127.0.0.1", broker_port),
            [("127.0.0.1", publisher_udp), ("127.0.0.1", consumer_udp)],
        )
        intro = nat.intro.getsockname()[1]
        (consumer_port,) = network["ports"](1)
        origin_port = network["serve"](
            stack, socketserver.ThreadingTCPServer(("127.0.0.1", 0), GreetingEcho)
        )
        broker_conf = f'bind="127.0.0.1:{broker_port}"\npeer_sha256=["{fingerprint("publisher")}","{fingerprint("consumer")}"]\n'
        broker = start("intro", "rendezvous", broker_conf)
        peers = []
        configs = {}
        for role, name, peer, port, udp in [
            ("publish", "publisher", "consumer", origin_port, publisher_udp),
            ("consume", "consumer", "publisher", consumer_port, consumer_udp),
        ]:
            configs[name] = (
                f'role="{role}"\nbind="127.0.0.1:{udp}"\nrendezvous="127.0.0.1:{intro}"\nrendezvous_name="localhost"\npeer_name="localhost"\npeer_sha256="{fingerprint(peer)}"\nlocal="127.0.0.1:{port}"\n'
            )
            peers.append(start(name, "peer", configs[name]))

        payload = network["PAYLOAD"]

        def echo(_):
            with socket.create_connection(
                ("127.0.0.1", consumer_port), timeout=5
            ) as client:
                assert network["receive"](client, 5) == b"hello"
                client.sendall(payload)
                client.shutdown(socket.SHUT_WR)
                assert network["receive"](client, len(payload)) == payload
                assert client.recv(1) == b""

        def await_echo(seconds):
            deadline = time.monotonic() + seconds
            while True:
                try:
                    echo(0)
                    return
                except (OSError, EOFError):
                    if time.monotonic() >= deadline:
                        raise
                    time.sleep(0.2)

        await_echo(30)
        broker.terminate()
        broker.wait(timeout=5)
        with ThreadPoolExecutor(max_workers=8) as pool:
            list(pool.map(echo, range(8)))
        assert all(nat.opened) and all(nat.direct_packets)
        print(
            f"Direct QUIC through two simulated port-restricted NATs passed server-first TCP, half-close and eight concurrent 256 KiB echoes after rendezvous shutdown. Forwarded UDP packets: {nat.direct_packets}"
        )
        start("intro", "rendezvous", broker_conf)
        peers[0].terminate()
        peers[0].wait(timeout=5)
        peers[0] = start("publisher", "peer", configs["publisher"])
        await_echo(75)
        print("The running consumer reconnected after the publisher restarted.")
        for proc in peers:
            proc.terminate()
            proc.wait(timeout=5)
        start("publisher", "peer", configs["publisher"])
        start(
            "consumer",
            "peer",
            configs["consumer"].replace(fingerprint("publisher"), fingerprint("intro")),
        )
        deadline = time.monotonic() + 20
        log = work / "consumer.log"
        while b"fingerprint mismatch" not in log.read_bytes():
            if time.monotonic() >= deadline:
                raise AssertionError(log.read_text(errors="replace"))
            time.sleep(0.1)
        with socket.create_connection(
            ("127.0.0.1", consumer_port), timeout=1
        ) as client:
            try:
                assert not client.recv(1), (
                    "Rejected peer unexpectedly reached the service"
                )
            except (TimeoutError, ConnectionResetError):
                pass
        print(
            "A CA-valid publisher with the wrong pinned identity was rejected before reaching the TCP service."
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--openssl", default=shutil.which("openssl") or "openssl")
    args = parser.parse_args()
    check(args.binary.resolve(), args.openssl)
