#!/usr/bin/env python3
"""Test the installed effect in a private virtual KWin, never on the user's display."""
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
SOCKET = "myconfig-glass-test"

if "--isolated" not in sys.argv:
    with tempfile.TemporaryDirectory(prefix="myconfig-glass-test-") as temporary:
        directory = Path(temporary)
        env = os.environ.copy()
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "QT_QPA_PLATFORM"):
            env.pop(key, None)
        for key, name in (("XDG_RUNTIME_DIR", "runtime"), ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache")):
            target = directory / name
            target.mkdir(mode=0o700)
            env[key] = str(target)
        env["MYCONFIG_GLASS_TEST_ROOT"] = temporary
        env["XDG_CONFIG_DIRS"] = "/etc/xdg"
        env["XDG_DATA_DIRS"] = "/usr/local/share:/usr/share"
        env["XDG_CURRENT_DESKTOP"] = ""
        env["QT_QPA_PLATFORMTHEME"] = "generic"
        env["QT_FORCE_STDERR_LOGGING"] = "1"
        effect_id = Path("/usr/share/myconfig/kde-glass/effect-id").read_text().strip()
        preset = (ROOT / "dotfiles/kde-plasma/.local/share/myconfig/kde-plasma/glass.conf").read_text()
        (directory / "config/kwinrc").write_text(
            preset + f"\n[Plugins]\nblurEnabled=false\n{effect_id}Enabled=true\n"
            "scaleEnabled=false\nfadeEnabled=false\nfullscreenEnabled=false\n"
        )
        flags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs", "Qt6Core", "Qt6Gui", "Qt6DBus"], text=True))
        subprocess.run(["c++", "-std=c++20", "-fPIC", str(ROOT / "test/test-kde-glass-capture.cpp"), "-o", str(directory / "capture"), *flags], check=True)
        result = subprocess.run(["dbus-run-session", "--", sys.executable, __file__, "--isolated"], env=env, timeout=60)
        sys.exit(result.returncode)

directory = Path(os.environ["MYCONFIG_GLASS_TEST_ROOT"])
assert Path(os.environ["XDG_RUNTIME_DIR"]) == directory / "runtime"
assert Path(os.environ["XDG_CONFIG_HOME"]) == directory / "config"
assert "DISPLAY" not in os.environ and "WAYLAND_DISPLAY" not in os.environ

from PySide6.QtCore import QObject, QUrl
from PySide6.QtGui import QGuiApplication, QImage
from PySide6.QtQml import QQmlApplicationEngine

with (directory / "kwin.log").open("w") as log:
    kwin = subprocess.Popen([
        "kwin_wayland", "--virtual", "--width", "640", "--height", "480",
        "--socket", SOCKET, "--no-global-shortcuts", "--no-lockscreen", "--no-kactivities", "--inputmethod", "/usr/bin/true",
    ], stdout=log, stderr=log, env={**os.environ, "KWIN_SCREENSHOT_NO_PERMISSION_CHECKS": "1"})
    try:
        deadline = time.monotonic() + 15
        while not (directory / "runtime" / SOCKET).exists():
            assert kwin.poll() is None, "Virtual KWin exited before creating its socket"
            assert time.monotonic() < deadline, "Virtual KWin startup timed out"
            time.sleep(0.1)
        os.environ["WAYLAND_DISPLAY"] = SOCKET
        os.environ["QT_QPA_PLATFORM"] = "wayland"
        app = QGuiApplication([])
        engine = QQmlApplicationEngine()
        engine.warnings.connect(lambda errors: print([error.toString() for error in errors], flush=True))
        engine.loadData(b"""
import QtQuick
import MyConfig.Glass 1.0
Window {
    width: 640; height: 480; visible: true
    flags: Qt.FramelessWindowHint
    title: "Synthetic background"
    Repeater {
        model: 4800
        Rectangle {
            required property int index
            x: (index % 80) * 8; y: Math.floor(index / 80) * 8
            width: 8; height: 8
            color: ((index % 80) + Math.floor(index / 80)) % 2 ? "white" : "black"
        }
    }
    Window {
        id: glass
        width: 320; height: 240; visible: true
        flags: Qt.Tool | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint
        color: "transparent"; title: "MyConfig Island Test"
        Rectangle { anchors.fill: parent; anchors.margins: 16; radius: 18; color: "transparent"; border.color: "white" }
        BlurRegion { objectName: "blur"; window: glass; rect: Qt.rect(16, 16, 288, 208); radius: 18; enabled: false }
    }
}
""", QUrl())
        deadline = time.monotonic() + 5
        while not engine.rootObjects() and time.monotonic() < deadline:
            app.processEvents()
            time.sleep(0.01)
        assert engine.rootObjects(), "Synthetic scene did not load"
        root = engine.rootObjects()[0]
        blur = root.findChild(QObject, "blur")
        assert blur is not None

        def settle():
            deadline = time.monotonic() + 1.5
            while time.monotonic() < deadline:
                app.processEvents()
                time.sleep(0.01)

        def capture():
            # The helper inherits only the private test compositor and private bus.
            image = QImage.fromData(subprocess.check_output([str(directory / "capture")], timeout=10))
            assert image.size().width() == 640 and image.size().height() == 480
            return image

        settle()
        plain = capture()
        blur.setProperty("enabled", True)
        settle()
        blurred = capture()
        effect_id = Path("/usr/share/myconfig/kde-glass/effect-id").read_text().strip()
        diagnostic = subprocess.check_output(["qdbus6", "org.kde.KWin", "/Effects", "org.kde.kwin.Effects.debug", effect_id, ""], text=True)
        print(diagnostic)
        changed = sum(abs(plain.pixelColor(x, y).red() - blurred.pixelColor(x, y).red()) > 20
                      for y in range(0, 480, 2) for x in range(0, 640, 2))
        assert changed > 500, f"Virtual KWin did not render blur/refraction: {changed} changed samples"
        blur.setProperty("enabled", False)
        settle()
        restored = capture()
        remaining = sum(abs(plain.pixelColor(x, y).red() - restored.pixelColor(x, y).red()) > 20
                        for y in range(0, 480, 2) for x in range(0, 640, 2))
        assert remaining < 10, f"Disabling blur left stale output: {remaining} samples"
        print(f"Virtual KWin integration passed: {changed} changed samples, no physical display used.")
        root.close()
    finally:
        kwin.terminate()
        try:
            kwin.wait(timeout=5)
        except subprocess.TimeoutExpired:
            kwin.kill()
            kwin.wait()
        print((directory / "kwin.log").read_text())
