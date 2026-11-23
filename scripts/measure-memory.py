"""Measure standalone client and gateway RSS with a token and mutual TLS on loopback.

Run with `uv run --with psutil scripts/measure-memory.py RELEASE_DIRECTORY`.
Each JSON row is one independent pair of release processes. Values are bytes.
"""

import argparse
import json
import os
import runpy
import secrets
import shutil
import socket
import socketserver
import statistics
import tempfile
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack
from pathlib import Path

import psutil

fixture = runpy.run_path(str(Path(__file__).with_name("check-tunnels.py")))
host = fixture["HOST"]


def sample(processes):
    readings = {name: [] for name in processes}
    for _ in range(20):
        time.sleep(0.1)
        for name, process in processes.items():
            readings[name].append(process.memory_info().rss)
    return {
        name: {"median": int(statistics.median(rss)), "maxSample": max(rss)}
        for name, rss in readings.items()
    }


def trial(binaries, work):
    control, mapped = fixture["ports"](2)
    token = secrets.token_hex(16)
    env = {
        **os.environ,
        "ASTRAVE_CONTROL_SOCKET": (
            rf"\\.\pipe\astrave-memory-{token}"
            if os.name == "nt"
            else str(work / "ctrl.sock")
        ),
    }
    gateway = (
        f'listen="{host}:{control}"\nproxyAddr="{host}"\nauth.token="{token}"\n'
        '[transport.tls]\nforce=true\ncertFile="server.pem"\nkeyFile="server.key"\ntrustedCaFile="ca.pem"\n'
    )
    with ExitStack() as stack:
        echo = fixture["serve"](
            stack, socketserver.ThreadingTCPServer((host, 0), fixture["Echo"])
        )
        agent = (
            f'server="{host}:{control}"\nauth.token="{token}"\n[transport.tls]\n'
            'enable=true\nserverName="pass.test"\ncertFile="client.pem"\nkeyFile="client.key"\ntrustedCaFile="ca.pem"\n'
            f'[[tunnels]]\nname="echo"\nprotocol="tcp"\nservice="{host}:{echo}"\nremotePort={mapped}\n'
        )

        def launch(name, config):
            return fixture["running"](binaries, work, name, config, name, env)

        with launch("astrave-server", gateway) as (server, _, _):
            time.sleep(1)
            idle = sample({"gateway": psutil.Process(server.pid)})
        client, _, log = stack.enter_context(launch("astrave", agent))
        time.sleep(1)
        processes = {"client": psutil.Process(client.pid)}
        idle.update(sample(processes))
        server, _, _ = stack.enter_context(launch("astrave-server", gateway))
        processes["gateway"] = psutil.Process(server.pid)
        with fixture["ready"](mapped, client, log) as sock:
            sock.sendall(b"ready")
            assert fixture["receive"](sock, 5) == b"ready"
        time.sleep(1)
        online = sample(processes)
        block = bytes(range(256)) * 256
        stop = threading.Event()
        started = threading.Barrier(9, timeout=15)

        def forward():
            with socket.create_connection((host, mapped), timeout=10) as conn:
                conn.sendall(block)
                assert fixture["receive"](conn, len(block)) == block
                started.wait()
                total = 0
                while not stop.is_set():
                    conn.sendall(block)
                    assert fixture["receive"](conn, len(block)) == block
                    total += len(block) * 2
                return total

        with ThreadPoolExecutor(max_workers=8) as pool:
            tasks = [pool.submit(forward) for _ in range(8)]
            try:
                started.wait()
                forwarding = sample(processes)
            finally:
                stop.set()
            verified = sum(task.result() for task in tasks)
        return {
            "idle": idle,
            "online": online,
            "forwarding": forwarding,
            "verifiedDuplexBytes": verified,
        }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", type=Path)
    parser.add_argument("--openssl", default=shutil.which("openssl") or "openssl")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="astrave-memory-") as temp:
        work = Path(temp)
        fixture["certificates"](work, args.openssl)
        for _ in range(3):
            print(json.dumps(trial(args.binaries.resolve(), work)), flush=True)
