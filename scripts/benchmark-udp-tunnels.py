"""Measure Windows process memory after verifying 128 independent UDP tunnels."""

import argparse
import json
import os
import runpy
import socket
import socketserver
import subprocess
import tempfile
import time
from contextlib import ExitStack
from pathlib import Path

helpers = runpy.run_path(str(Path(__file__).with_name("check-tunnels.py")))
HOST = "127.0.0.1"


def verify_echo(sock, port, payload):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        sock.sendto(payload, (HOST, port))
        try:
            reply, source = sock.recvfrom(2048)
        except (TimeoutError, ConnectionResetError):
            continue
        if source == (HOST, port):
            if reply != payload:
                raise ValueError(f"Unexpected payload on UDP port {port}")
            return
    raise TimeoutError(f"No echo on UDP port {port}")


def trial(binaries):
    with tempfile.TemporaryDirectory() as tmp, ExitStack() as stack:
        work = Path(tmp)
        control, = helpers["ports"](1)
        mapped = helpers["ports"](128, socket.SOCK_DGRAM)
        echo = helpers["serve"](stack, socketserver.UDPServer((HOST, 0), helpers["UdpEcho"]))
        config = f'server="{HOST}:{control}"\nudpPacketSize=65535\ntransport.tls.enable=false\n'
        for i, port in enumerate(mapped):
            config += (
                f'[[tunnels]]\nname="echo{i}"\nprotocol="udp"\n'
                f'service="{HOST}:{echo}"\nremotePort={port}\n'
            )
        env = {**os.environ, "ASTRAVE_CONTROL_SOCKET": rf"\\.\pipe\astrave-bench-{os.getpid()}"}
        server, _, _ = stack.enter_context(helpers["running"](
            binaries, work, "astrave-server",
            f'listen="{HOST}:{control}"\nproxyAddr="{HOST}"\nudpPacketSize=65535\n', "gateway", env,
        ))
        client, _, _ = stack.enter_context(helpers["running"](binaries, work, "astrave", config, "client", env))
        sock = stack.enter_context(socket.socket(type=socket.SOCK_DGRAM))
        sock.settimeout(.25)
        for i, port in enumerate(mapped):
            verify_echo(sock, port, bytes([i]) * 256)
        time.sleep(.5)
        rows = json.loads(subprocess.check_output([
            "powershell", "-NoProfile", "-Command",
            f"Get-Process -Id {server.pid},{client.pid} | Select-Object Id,WorkingSet64,PrivateMemorySize64 | ConvertTo-Json -Compress",
        ]))
        result = {"verifiedTunnels": len(mapped)}
        for row in rows:
            result["gateway" if row["Id"] == server.pid else "client"] = {
                "workingSetBytes": row["WorkingSet64"], "privateBytes": row["PrivateMemorySize64"],
            }
        return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    args = parser.parse_args()
    for build in ("before", "after", "after", "before", "before", "after"):
        print(json.dumps({"build": build, **trial(getattr(args, build).resolve())}), flush=True)
