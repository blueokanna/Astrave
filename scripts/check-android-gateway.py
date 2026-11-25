"""Exercise client, gateway restart and payload integrity entirely on an Android device."""

import argparse
import random
import runpy
import secrets
import subprocess
import tempfile
import time
from pathlib import Path

fixture = runpy.run_path(str(Path(__file__).with_name("check-tunnels.py")))


def check(serial, binaries, openssl, protocol):
    def adb(*args):
        subprocess.run(["adb", "-s", serial, *args], check=True, timeout=180)

    token = secrets.token_hex(16)
    control, udp, mapped, echo = random.SystemRandom().sample(range(20000, 60000), 4)
    endpoint = control if protocol == "tcp" else udp
    udp_listener = "" if protocol == "tcp" else f"{protocol}Port={udp}\n"
    remote = f"/data/local/tmp/astrave-{token}"
    with tempfile.TemporaryDirectory() as temp:
        work = Path(temp)
        fixture["certificates"](work, openssl)
        (work / "payload").write_bytes(fixture["PAYLOAD"])
        (work / "gateway.toml").write_text(
            f'listen="[::1]:{control}"\n{udp_listener}proxyAddr="127.0.0.1"\nauth.token="{token}"\n'
            '[transport.tls]\nforce=true\ncertFile="server.pem"\nkeyFile="server.key"\ntrustedCaFile="ca.pem"\n'
        )
        (work / "agent.toml").write_text(
            f'server="localhost:{endpoint}"\ntransport.protocol="{protocol}"\nauth.token="{token}"\n'
            '[transport.tls]\nserverName="pass.test"\ntrustedCaFile="ca.pem"\ncertFile="client.pem"\nkeyFile="client.key"\n'
            f'[[tunnels]]\nname="echo"\nprotocol="tcp"\nservice="127.0.0.1:{echo}"\nremotePort={mapped}\n'
        )
        (work / "run.sh").write_text(
            f"""set -eu
cd {remote}
export ASTRAVE_CONTROL_SOCKET=$PWD/ctl.sock
gateway= agent= origin=
cleanup() {{
    for pid in $gateway $agent $origin; do kill "$pid" 2>/dev/null || true; done
    wait || true
}}
trap cleanup EXIT
toybox nc -L -s 127.0.0.1 -p {echo} /system/bin/cat >origin.log 2>&1 & origin=$!
./astrave -c agent.toml >agent.log 2>&1 & agent=$!
for instance in 1 2; do
    ./astrave-server -c gateway.toml >gateway.log 2>&1 & gateway=$!
    attempts=0
    until toybox nc -w 1 -W 3 127.0.0.1 {mapped} <payload >result 2>/dev/null && cmp -s payload result; do
        attempts=$((attempts + 1))
        if [ "$attempts" -ge 120 ] || ! kill -0 "$agent" "$gateway" "$origin"; then
            cat agent.log gateway.log origin.log; exit 1
        fi
        sleep 1
    done
    echo "Client PID $agent verified 256 KiB with gateway instance $instance"
    kill "$gateway"; wait "$gateway" || true; gateway=
done
""",
            newline="\n",
        )
        adb("shell", f"umask 077 && mkdir {remote}")
        try:
            for name in [
                "agent.toml",
                "gateway.toml",
                "server.pem",
                "server.key",
                "client.pem",
                "client.key",
                "ca.pem",
                "payload",
                "run.sh",
            ]:
                adb("push", str(work / name), remote + "/" + name)
            for name in ["astrave", "astrave-server"]:
                adb("push", str(binaries / name), remote + "/" + name)
            start = time.monotonic()
            adb(
                "shell", f"cd {remote} && chmod 700 astrave astrave-server && sh run.sh"
            )
            print(
                f"{protocol} Android loopback test passed in {time.monotonic() - start:.1f}s",
                flush=True,
            )
        finally:
            adb("shell", f"rm -rf {remote}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("serial")
    parser.add_argument("binaries", type=Path)
    parser.add_argument("--openssl", default="openssl")
    parser.add_argument("--protocol", choices=("tcp", "quic", "kcp"), default="tcp")
    args = parser.parse_args()
    check(args.serial, args.binaries, args.openssl, args.protocol)
