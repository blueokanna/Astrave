"""Time HTTP gateway registration and authenticated route selection on loopback."""

import argparse
import json
import os
import runpy
import statistics
import subprocess
import tempfile
import time
from contextlib import ExitStack
from pathlib import Path

helpers = runpy.run_path(str(Path(__file__).with_name("check-tunnels.py")))
running, ports, http = (helpers[k] for k in ("running", "ports", "get_http"))


def trial(binaries):
    domains = [f"host-{i}.test" for i in range(8)]
    paths = [f"/prefix-{i:04d}/" for i in range(512)]
    control, gw = ports(2)
    cfg = (
        f'server="127.0.0.1:{control}"\ntransport.tls.enable=false\n'
        '[[tunnels]]\nname="routes"\nprotocol="http"\nservice="127.0.0.1:1"\n'
        'basicAuthUser="u"\nbasicAuthPassword="password"\n'
        f"domains={json.dumps(domains)}\nlocations={json.dumps(paths)}\n"
    )
    with tempfile.TemporaryDirectory() as tmp, ExitStack() as stack:
        work = Path(tmp)
        env = {
            **os.environ,
            "RUST_LOG": "warn",
            "ASTRAVE_CONTROL_SOCKET": rf"\\.\pipe\astrave-routes-{os.getpid()}"
            if os.name == "nt"
            else str(work / "client.sock"),
        }
        server, _, _ = stack.enter_context(
            running(
                binaries,
                work,
                "astrave-server",
                f'listen="127.0.0.1:{control}"\nproxyAddr="127.0.0.1"\nhttpGwPort={gw}\n',
                "server",
                env,
            )
        )

        def await_status(host, path, status):
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                try:
                    if http(gw, host, path)[0] == status:
                        return
                except OSError:
                    pass
                time.sleep(0.002)
            raise TimeoutError(f"{host}{path} did not return {status}")

        await_status(domains[0], paths[0], 404)
        start = time.perf_counter_ns()
        stack.enter_context(running(binaries, work, "astrave", cfg, "client", env))
        await_status(domains[-1], paths[-1], 401)
        registration_ms = (time.perf_counter_ns() - start) / 1e6
        memory = {}
        if os.name == "nt":
            memory = json.loads(
                subprocess.check_output(
                    [
                        "powershell",
                        "-NoProfile",
                        "-Command",
                        f"Get-Process -Id {server.pid} | Select-Object WorkingSet64,PrivateMemorySize64 | ConvertTo-Json -Compress",
                    ]
                )
            )
        hits, misses = [], []
        for i in range(1024):
            present = i % 2 == 0
            path = (
                paths[((i // 2) * 37) % len(paths)] + "child" if present else "/absent"
            )
            start = time.perf_counter_ns()
            status = http(gw, domains[(i // 2) % len(domains)], path)[0]
            elapsed = time.perf_counter_ns() - start
            assert status == (401 if present else 404), (path, status)
            (hits if present else misses).append(elapsed / 1000)
        return {
            "domains": len(domains),
            "locationsPerDomain": len(paths),
            "readyMs": registration_ms,
            "server": memory,
            "hitP50Us": statistics.median(hits),
            "missP50Us": statistics.median(misses),
        }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    args = parser.parse_args()
    for name in [
        "baseline",
        "candidate",
        "candidate",
        "baseline",
        "baseline",
        "candidate",
    ]:
        print(
            json.dumps({"build": name, **trial(getattr(args, name).resolve())}),
            flush=True,
        )
