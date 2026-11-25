"""Exercise the libp2p CLI with independent identities, real sockets and no cloud account."""

import argparse
import os
import runpy
import socket
import socketserver
import subprocess
import tempfile
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack, contextmanager
from pathlib import Path

fixture = runpy.run_path(str(Path(__file__).with_name("check-tunnels.py")))


class Origin(fixture["Echo"]):
    def handle(self):
        self.server.contacted.set()
        self.request.sendall(b"hello")
        super().handle()


@contextmanager
def endpoint(binary, profile):
    logfile = profile.with_suffix(".log")
    with logfile.open("wb") as log:
        proc = subprocess.Popen(
            [binary, "p2p", "-c", profile],
            stdout=log,
            stderr=log,
            env={**os.environ, "RUST_LOG": "astrave=debug"},
        )
    try:
        yield proc
    except BaseException:
        print(logfile.read_text(errors="replace"))
        raise
    finally:
        proc.terminate()
        proc.wait(timeout=5)


def measure_echo(transport, echo, processes):
    import psutil

    peers = [psutil.Process(p.pid) for p in processes]
    idle = [p.memory_info().rss for p in peers]
    peak = idle.copy()
    start = time.monotonic()
    deadline = start + 5

    def traffic():
        count = 0
        while time.monotonic() < deadline:
            echo()
            count += 1
        return count

    with ThreadPoolExecutor(max_workers=8) as pool:
        workers = [pool.submit(traffic) for _ in range(8)]
        while not all(f.done() for f in workers):
            peak = [max(prev, p.memory_info().rss) for prev, p in zip(peak, peers)]
            time.sleep(0.05)
        count = sum(f.result() for f in workers)
    seconds = time.monotonic() - start
    mib = len(fixture["PAYLOAD"]) * count / (1 << 20)
    print(
        f"{transport}: {count} echoes in {seconds:.2f}s, {mib / seconds:.2f} MiB/s verified return payload; "
        f"RSS bytes [publisher, consumer] idle={idle}, sampled max={peak}",
        flush=True,
    )


def exercise(binary, work, transport, measure):
    direct, consumer, unavailable = fixture["ports"](3)
    suffix = (
        f"tcp/{direct}"
        if transport == "tcp"
        else f"udp/{fixture['ports'](1, socket.SOCK_DGRAM)[0]}/quic-v1"
    )
    address = f'"/ip4/127.0.0.1/{suffix}"'
    ids = {}
    for name in ("publish", "consume", "other"):
        key = work / f"{name}.key"
        ids[name] = subprocess.check_output(
            [binary, "p2p-keygen", key], text=True
        ).strip()
        before = key.read_bytes()
        duplicate = subprocess.run(
            [binary, "p2p-keygen", key], capture_output=True, check=False
        )
        assert duplicate.returncode != 0
        assert key.read_bytes() == before

    with ExitStack() as stack:
        server = socketserver.ThreadingTCPServer(("127.0.0.1", 0), Origin)
        server.contacted = threading.Event()
        origin = fixture["serve"](stack, server)

        def profile(role, peer_id):
            path = work / f"{role}.toml"
            addresses = (
                address
                if role == "publish"
                else f'"/ip4/127.0.0.1/tcp/{unavailable}", {address}'
            )
            port = origin if role == "publish" else consumer
            path.write_text(
                f'role="{role}"\nidentityFile="{role}.key"\npeerId="{peer_id}"\nlocal="127.0.0.1:{port}"\naddresses=[{addresses}]\n'
            )
            return path

        def echo():
            with socket.create_connection(("127.0.0.1", consumer), timeout=5) as conn:
                assert fixture["receive"](conn, 5) == b"hello"
                conn.sendall(fixture["PAYLOAD"])
                conn.shutdown(socket.SHUT_WR)
                assert (
                    fixture["receive"](conn, len(fixture["PAYLOAD"]))
                    == fixture["PAYLOAD"]
                )
                assert conn.recv(1) == b""

        def ready(*processes):
            deadline = time.monotonic() + 25
            while time.monotonic() < deadline:
                assert all(p.poll() is None for p in processes), "endpoint exited"
                try:
                    echo()
                    return
                except (OSError, EOFError):
                    time.sleep(0.1)
            raise RuntimeError("libp2p link did not become ready")

        for expected_consumer, expected_publisher in (
            ("other", "publish"),
            ("consume", "other"),
        ):
            with (
                endpoint(binary, profile("publish", ids[expected_consumer])) as pub,
                endpoint(binary, profile("consume", ids[expected_publisher])) as sub,
            ):
                deadline = time.monotonic() + 5
                while True:
                    try:
                        conn = socket.create_connection(
                            ("127.0.0.1", consumer), timeout=1
                        )
                        break
                    except ConnectionRefusedError:
                        if time.monotonic() >= deadline:
                            raise
                        time.sleep(0.05)
                with conn:
                    conn.sendall(b"denied")
                    rejecting = "publish" if expected_consumer == "other" else "consume"
                    log = work / f"{rejecting}.log"
                    deadline = time.monotonic() + 15
                    while b"unexpected remote Peer ID" not in log.read_bytes():
                        assert time.monotonic() < deadline, (
                            f"{rejecting} did not reject the peer"
                        )
                        time.sleep(0.05)
                    assert not server.contacted.is_set(), (
                        "unexpected identity reached the origin"
                    )
                assert pub.poll() is None and sub.poll() is None

        with endpoint(binary, profile("consume", ids["publish"])) as sub:
            publish = profile("publish", ids["consume"])
            with endpoint(binary, publish) as pub:
                ready(pub, sub)
                with ThreadPoolExecutor(max_workers=8) as pool:
                    futures = [pool.submit(echo) for _ in range(8)]
                    for future in futures:
                        future.result()
                if measure:
                    measure_echo(transport, echo, [pub, sub])
            with endpoint(binary, publish) as pub:
                ready(pub, sub)
    print(
        f"{transport}: mutual pinning, address fallback, server-first data, half-close, eight concurrent 256 KiB echoes and publisher restart passed",
        flush=True,
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument(
        "--measure",
        action="store_true",
        help="sample process RSS and loopback echo throughput (requires psutil)",
    )
    args = parser.parse_args()
    for transport in ("tcp", "quic"):
        with tempfile.TemporaryDirectory() as directory:
            exercise(args.binary.resolve(), Path(directory), transport, args.measure)
