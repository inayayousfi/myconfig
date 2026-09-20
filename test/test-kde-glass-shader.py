#!/usr/bin/env python3
"""Render a synthetic scene offscreen, never capture the desktop.

Argument: the patched kwin-effects-glass source directory.
"""
import os
from pathlib import Path
import sys

os.environ["QT_QPA_PLATFORM"] = "offscreen"

from PySide6.QtCore import QSize
from PySide6.QtGui import QColor, QGuiApplication, QOffscreenSurface, QOpenGLContext, QSurfaceFormat, QVector2D, QVector3D, QVector4D
from PySide6.QtOpenGL import QOpenGLFramebufferObject, QOpenGLShader, QOpenGLShaderProgram, QOpenGLTexture, QOpenGLVertexArrayObject
from PySide6.QtGui import QImage

app = QGuiApplication([])
fmt = QSurfaceFormat()
fmt.setVersion(3, 3)
fmt.setProfile(QSurfaceFormat.CoreProfile)
context = QOpenGLContext()
context.setFormat(fmt)
assert context.create(), "Cannot create offscreen OpenGL context"
surface = QOffscreenSurface()
surface.setFormat(context.format())
surface.create()
assert context.makeCurrent(surface)
functions = context.functions()
functions.initializeOpenGLFunctions()

source = Path(sys.argv[1]) / "src/shaders"
glass = (source / "glass.glsl").read_text().replace('#include "snells-glass.glsl"', (source / "snells-glass.glsl").read_text())
program = QOpenGLShaderProgram()
vertex = """#version 330 core
out vec2 uv;
void main() {
    vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
    uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"""
fragment = """#version 330 core
uniform sampler2D texUnit;
uniform sampler2D sceneTexture;
uniform vec2 blurSize;
uniform vec4 cornerRadius;
in vec2 uv;
out vec4 fragColor;
""" + glass + "\nvoid main() { fragColor = glass(texture(texUnit, uv), cornerRadius); }"
assert program.addShaderFromSourceCode(QOpenGLShader.Vertex, vertex), program.log()
assert program.addShaderFromSourceCode(QOpenGLShader.Fragment, fragment), program.log()
assert program.link(), program.log()
assert program.bind()

image = QImage(256, 128, QImage.Format_RGBA8888)
for y in range(image.height()):
    for x in range(image.width()):
        image.setPixelColor(x, y, QColor("white" if (x // 8 + y // 8) % 2 else "black"))
scene = QOpenGLTexture(image)
diffuse = QOpenGLTexture(image)
scene.setWrapMode(QOpenGLTexture.ClampToEdge)
diffuse.setWrapMode(QOpenGLTexture.ClampToEdge)
diffuse.bind(0)
scene.bind(1)
for name, value in {
    "texUnit": 0, "sceneTexture": 1, "blurSize": QVector2D(256, 128),
    "cornerRadius": QVector4D(18, 18, 18, 18), "edgeSizePixels": 160.0,
    "materialThickness": 80.0,
    "materialCurvature": 1.25,
    "refractionNormalPow": 1.0, "refractionRGBFringing": 0.0,
    "refractionOffsetStrength": 1.0, "refractionBevelIntensity": 1.0,
    "physicallyBasedRefraction": 1, "edgeLighting": 0,
    "tintStrength": 0.0, "tintColor": QVector3D(0, 0, 0), "tintGray": 0.0,
    "autoTintAlpha": 0, "autoTintAlphaRange": QVector2D(0, 1),
    "glowStrength": 0.0, "glowColor": QVector3D(0, 0, 0),
}.items():
    location = program.uniformLocation(name)
    if isinstance(value, int):
        functions.glUniform1i(location, value)
    elif isinstance(value, float):
        functions.glUniform1f(location, value)
    else:
        program.setUniformValue(location, value)
assert functions.glGetError() == 0, "OpenGL uniform setup failed"

vao = QOpenGLVertexArrayObject()
assert vao.create()
vao.bind()
fbo = QOpenGLFramebufferObject(QSize(256, 128))
assert fbo.isValid()

def render(strength):
    assert fbo.bind()
    assert program.bind()
    vao.bind()
    diffuse.bind(0)
    scene.bind(1)
    functions.glViewport(0, 0, 256, 128)
    functions.glClearColor(0, 0, 0, 0)
    functions.glClear(0x4000)
    functions.glUniform1f(program.uniformLocation("refractionStrength"), strength)
    functions.glUniform1f(program.uniformLocation("materialIOR"), 1.5)
    functions.glUniform1f(program.uniformLocation("materialRoughness"), 0.0)
    functions.glUniform1f(program.uniformLocation("materialInteriorShadow"), 0.0)
    functions.glDrawArrays(0x0004, 0, 3)
    assert functions.glGetError() == 0, "Offscreen draw failed"
    return fbo.toImage()

plain = render(0.0)
liquid = render(0.75)

def differences(first, second, rows, columns=range(32, 224)):
    return sum(abs(first.pixelColor(x, y).red() - second.pixelColor(x, y).red()) > 20
               for y in rows for x in columns)

assert differences(plain, liquid, range(4, 20)) > 100, "LiquidGlass radial remap did not refract the edge"
assert differences(plain, liquid, range(62, 66), range(126, 130)) < 6, "LiquidGlass radial remap moved the center anchor"
assert max(liquid.pixelColor(x, y).alpha() for y in range(1, 4) for x in range(1, 4)) < 8, \
    "LiquidGlass did not make pixels outside its adaptive rounded rectangle transparent"
assert "OverShifted/LiquidGlass" in glass, "LiquidGlass source attribution is missing"
assert "aspectRatio" in glass and "dynamicRadius" in glass, "LiquidGlass rounding is not derived from its allocation"
assert "roundedRectangleDist(position, halfBlurSize" in glass, "LiquidGlass does not fill an adaptive rounded rectangle"
assert "1.0 - b * pow(c * liquidGlassE, -d * distance - a)" in glass, "LiquidGlass radial curve is missing"
assert "vec3(-0.70, 0.70, 0.72)" in glass, "Top-left material light is missing"
assert "edgeRelease" in glass and "shoulderLight" in glass, "Material light is not positioned on the lens shoulder"
assert "edgeKiss" in glass and "lightProfile" in glass, "Material light does not touch the silhouette"
assert "silhouetteKiss" in glass and "edgeSpecular" in glass, "Material light does not compensate at alpha-covered edge pixels"
assert "oppositeLight" in glass and "respondingShadow" in glass, "Material light has no opposing directional shadow"
assert "roughTransmission" in glass and "materialRoughness" in glass, "Material surface roughness is missing"
print("OverShifted LiquidGlass shader checks passed: refraction, grain, alpha, and top-left transmitted light.")

fbo.release()
vao.release()
program.release()
scene.destroy()
diffuse.destroy()
vao.destroy()
