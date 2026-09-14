#!/usr/bin/env python3
"""Run with QT_QPA_PLATFORM=offscreen after building/installing MyConfig.Glass."""

import os
from pathlib import Path
import time
import xml.etree.ElementTree as ET

os.environ["QT_QPA_PLATFORM"] = "offscreen"

from PySide6.QtCore import QObject, QRectF, QUrl
from PySide6.QtGui import QGuiApplication
from PySide6.QtQml import QQmlComponent, QQmlEngine
from PySide6.QtQuick import QQuickWindow, QSGRendererInterface

root = Path(__file__).resolve().parents[1]
plasma = root / "dotfiles/kde-plasma/.local/share"
QQuickWindow.setSceneGraphBackend("rhi")
QQuickWindow.setGraphicsApi(QSGRendererInterface.OpenGL)
app = QGuiApplication([])
engine = QQmlEngine()
component = QQmlComponent(engine)
component.setData(b"""
import QtQuick
import MyConfig.Glass 1.0
Window {
    id: testWindow
    width: 200; height: 100
    BlurRegion { objectName: "blur"; window: testWindow; rect: Qt.rect(16, 16, 168, 68); radius: 18 }
}
""", QUrl())
assert not component.isError(), [error.toString() for error in component.errors()]
window = component.create()
assert window is not None, [error.toString() for error in component.errors()]
blur = window.findChild(QObject, "blur")
assert blur is not None
for visible in (True, False, True):
    window.setVisible(visible)
    app.processEvents()
    for radius, rect in ((18, QRectF(16, 16, 168, 68)), (30, QRectF(16, 16, 680, 536)), (0, QRectF())):
        blur.setProperty("radius", radius)
        blur.setProperty("rect", rect)
        app.processEvents()
blur.setProperty("enabled", False)
window.close()
app.processEvents()

# Resolve all QML imports and parse the complete island without showing a desktop window.
island = QQmlComponent(engine, QUrl.fromLocalFile(str(plasma / "plasma/plasmoids/myconfig.island/contents/ui/main.qml")))
assert not island.isError(), [error.toString() for error in island.errors()]

source = (plasma / "plasma/plasmoids/myconfig.island/contents/ui/main.qml").read_text()
shadow_start = source.index("            Item {\n                id: shadowSource")
shadow_end = source.index("            Controls.Button {", shadow_start)
shadow_component = QQmlComponent(engine)
shadow_component.setData(("""
import QtQuick
import org.kde.kirigami as Kirigami
import Qt5Compat.GraphicalEffects as GraphicalEffects
Window {
    width: 200; height: 120; color: "transparent"
    Rectangle { id: surface; x: 16; y: 16; width: 168; height: 88; radius: 18; color: "transparent" }
""" + source[shadow_start:shadow_end] + "}").encode(), QUrl())
assert not shadow_component.isError(), [error.toString() for error in shadow_component.errors()]
shadow_window = shadow_component.create()
assert isinstance(shadow_window, QQuickWindow)
shadow_window.show()
deadline = time.monotonic() + 0.3
while time.monotonic() < deadline:
    app.processEvents()
    time.sleep(0.01)
shadow_image = shadow_window.grabWindow()
assert not shadow_image.isNull(), "Offscreen shadow render failed"
assert shadow_image.pixelColor(100, 60).alpha() == 0, "Shadow tinted the glass interior"
assert max(shadow_image.pixelColor(100, y).alpha() for y in range(105, 119)) > 5, "Exterior shadow was not drawn"
shadow_window.close()
app.processEvents()

theme = plasma / "plasma/desktoptheme/blacknpink"
for name in ("widgets/panel-background.svg", "dialogs/background.svg", "solid/dialogs/background.svg"):
    svg = ET.parse(theme / name)
    elements = {element.get("id"): element for element in svg.iter()}
    for part in ("top", "bottom", "left", "right", "center", "topleft", "topright", "bottomleft", "bottomright"):
        assert part in elements and f"mask-{part}" in elements
    assert elements["shadow-hint-top-margin"].get("height") == "6"
    assert elements["shadow-hint-bottom-margin"].get("height") == "14"
assert (theme / "solid/dialogs/background.svg").resolve() == (theme / "dialogs/background.svg").resolve()
print("Glass module, island QML imports, and panel/dialog SVG checks passed.")
