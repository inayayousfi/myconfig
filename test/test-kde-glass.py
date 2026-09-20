#!/usr/bin/env python3
"""Run with QT_QPA_PLATFORM=offscreen after building/installing MyConfig.Glass."""

import os
from pathlib import Path
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
assert "id: shadowSource" not in source, "Island still draws an external rounded shadow"
assert "border." not in source, "Island still contains a mismatched QML border"
assert "radius: 0" in source, "Island does not submit a rectangular shader-owned blur region"

theme = plasma / "plasma/desktoptheme/blacknpink"
for name in ("widgets/panel-background.svg", "dialogs/background.svg", "solid/dialogs/background.svg"):
    svg = ET.parse(theme / name)
    elements = {element.get("id"): element for element in svg.iter()}
    for part in ("top", "bottom", "left", "right", "center", "topleft", "topright", "bottomleft", "bottomright"):
        assert part in elements and f"mask-{part}" in elements
    assert not any(element.get("id", "").startswith("shadow-") for element in elements.values()), \
        f"{name} still contains an external SVG shadow"
    if name == "widgets/panel-background.svg":
        assert not any("stroke" in element.attrib for element in elements.values()), \
            "floating panel still draws an exterior SVG outline"
assert (theme / "solid/dialogs/background.svg").resolve() == (theme / "dialogs/background.svg").resolve()
print("Glass module, island QML imports, and panel/dialog SVG checks passed.")
