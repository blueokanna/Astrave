"""Exercise a release APK on an English-language Android emulator."""

import argparse
import re
import subprocess
import time
import xml.etree.ElementTree as ET
from pathlib import Path


def check(apk, output, serial, extended=False):
    output.mkdir(parents=True, exist_ok=True)
    prefix = ["adb", "-s", serial] if serial else ["adb", "-e"]
    package = "dev.astrave.astrave"
    activity = package + "/io.flutter.embedding.android.FlutterActivity"

    def adb(*args, timeout=35):
        return subprocess.check_output([*prefix, *map(str, args)], timeout=timeout)

    def screen(name):
        data = adb("exec-out", "screencap", "-p")
        assert data.startswith(b"\x89PNG")
        (output / (name + ".png")).write_bytes(data)

    def locate(label, tap=False):
        until = time.monotonic() + 100
        while time.monotonic() < until:
            path = "/data/local/tmp/astrave-ui.xml"
            try:
                dump = adb("shell", "uiautomator", "dump", path)
                if b"dumped to:" not in dump:
                    continue
                xml = adb("exec-out", "cat", path)
                (output / "last.xml").write_bytes(xml)
                for n in ET.fromstring(xml).iter("node"):
                    if n.get("package") != package:
                        continue
                    if any(
                        label in n.get(k, "").splitlines()
                        for k in ("content-desc", "text")
                    ):
                        if tap:
                            x, y, x2, y2 = map(int, re.findall(r"\d+", n.get("bounds")))
                            adb("shell", "input", "tap", (x + x2) // 2, (y + y2) // 2)
                        return
            except (ET.ParseError, subprocess.SubprocessError):
                pass
            time.sleep(0.4)
        screen("failed")
        raise TimeoutError(f"Android accessibility label {label!r}")

    def choose(setting, option):
        locate(setting, True)
        locate(option, True)
        locate(setting)

    install = subprocess.run(
        [*prefix, "install", "--no-streaming", "-r", str(apk)],
        capture_output=True, text=True, errors="replace", timeout=240,
    )
    if install.returncode or "Success" not in install.stdout:
        raise RuntimeError(f"adb install ({install.returncode}): {install.stdout} {install.stderr}")
    adb("shell", "input", "keyevent", "224")
    adb("shell", "wm", "dismiss-keyguard")
    adb("shell", "am", "start", "-n", activity)
    locate("Settings", True)
    locate("Navigation capsule")
    for style, filename in [
        ("Aero Glass style", "aero"),
        ("Liquid Glass style", "liquid"),
        ("Gaussian blur", "settings-en"),
    ]:
        choose("Navigation capsule", style)
        locate(style)
        screen(filename)
    choose("Theme", "Dark")
    choose("Color palette", "Jade")
    if extended:
        choose("Theme", "AMOLED Monet")
        locate("AMOLED Monet")
        screen("settings-monet")
        locate("Language", True)
        locate("العربية", True)
        locate("AMOLED Monet")
        screen("settings-ar")
        locate("اللغة", True)
        locate("English", True)
        choose("Theme", "AMOLED")
    locate("Language", True)
    locate("简体中文", True)
    locate("青玉")
    screen("settings-zh")
    locate("证书", True)
    locate("DNS 服务商")
    screen("certificates-zh")
    adb("shell", "input", "keyevent", "4")
    locate("导航胶囊背景")
    adb("shell", "am", "force-stop", package)
    adb("shell", "am", "start", "-n", activity)
    locate("设置", True)
    for value in ["青玉", "高斯模糊", "AMOLED" if extended else "深色", "简体中文"]:
        locate(value)
    print(
        "Verified all glass selections, certificate navigation and persisted language, palette and theme."
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--serial")
    parser.add_argument("--extended", action="store_true", help="Exercise AMOLED and Arabic in builds with multilingual settings")
    args = parser.parse_args()
    check(args.apk, args.output, args.serial, args.extended)
