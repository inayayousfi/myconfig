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

def pattern(width, height, color):
    image = QImage(width, height, QImage.Format_RGBA8888)
    for y in range(height):
        for x in range(width):
            image.setPixelColor(x, y, QColor(color(x, y)))
    texture = QOpenGLTexture(image)
    texture.setWrapMode(QOpenGLTexture.ClampToEdge)
    return texture

uniforms = {
    "texUnit": 0, "sceneTexture": 1,
    "cornerRadius": QVector4D(18, 18, 18, 18), "edgeSizePixels": 160.0,
    "materialThickness": 80.0, "materialCurvature": 1.0, "materialIOR": 1.5,
    "materialRoughness": 0.0, "materialInteriorShadow": 0.0,
    "refractionNormalPow": 1.0, "refractionRGBFringing": 0.0,
    "refractionOffsetStrength": 1.0, "refractionBevelIntensity": 1.0,
    "physicallyBasedRefraction": 1, "edgeLighting": 0,
    "tintStrength": 0.0, "tintColor": QVector3D(0, 0, 0), "tintGray": 0.0,
    "autoTintAlpha": 0, "autoTintAlphaRange": QVector2D(0, 1),
    "glowStrength": 0.0, "glowColor": QVector3D(0, 0, 0),
}

vao = QOpenGLVertexArrayObject()
assert vao.create()

def render(texture, width, height, strength):
    fbo = QOpenGLFramebufferObject(QSize(width, height))
    assert fbo.isValid() and fbo.bind()
    assert program.bind()
    vao.bind()
    texture.bind(0)
    texture.bind(1)
    for name, value in {**uniforms, "blurSize": QVector2D(width, height), "refractionStrength": strength}.items():
        location = program.uniformLocation(name)
        if isinstance(value, int):
            functions.glUniform1i(location, value)
        elif isinstance(value, float):
            functions.glUniform1f(location, value)
        else:
            program.setUniformValue(location, value)
    functions.glViewport(0, 0, width, height)
    functions.glClearColor(0, 0, 0, 0)
    functions.glClear(0x4000)
    functions.glDrawArrays(0x0004, 0, 3)
    assert functions.glGetError() == 0, "Offscreen draw failed"
    image = fbo.toImage(True, 0).convertToFormat(QImage.Format_RGBA8888_Premultiplied)
    fbo.release()
    return image

checker = pattern(256, 128, lambda x, y: "white" if (x // 8 + y // 8) % 2 else "black")
plain = render(checker, 256, 128, 0.0)
liquid = render(checker, 256, 128, 0.75)

def differences(first, second, rows, columns=range(32, 224)):
    return sum(abs(first.pixelColor(x, y).red() - second.pixelColor(x, y).red()) > 20
               for y in rows for x in columns)

assert differences(plain, liquid, range(4, 20)) > 100, "Glass did not refract the edge"
# The face is lit and dimmed, so compare which squares are light, not values.
center = [(x, y) for y in range(60, 68) for x in range(124, 132)]
levels = [liquid.pixelColor(x, y).red() for x, y in center]
midpoint = (min(levels) + max(levels)) / 2
assert max(levels) - min(levels) > 60 and all(
    (plain.pixelColor(x, y).red() > 127) == (level > midpoint)
    for (x, y), level in zip(center, levels)), "Glass moved the center anchor"
assert max(liquid.pixelColor(x, y).alpha() for y in range(1, 4) for x in range(1, 4)) < 8, \
    "Glass did not make pixels outside its adaptive rounded rectangle transparent"

# Premultiplied output must never carry more color than coverage, or KWin's
# GL_ONE blend adds stray light over the desktop.
white = pattern(64, 64, lambda x, y: "white")
for image in (liquid, render(white, 256, 128, 0.75)):
    # pixelColor() unpremultiplies, so read the stored RGBA bytes instead.
    data = bytes(image.constBits())[:image.sizeInBytes()]
    assert all(max(data[i:i + 3]) <= data[i + 3] for i in range(0, len(data), 4)), \
        "Glass produced premultiplied color brighter than its coverage"

# Rounded glass refracts rather than stretches: its rim compresses the
# background and its face magnifies it. Through horizontal stripes 4 px tall,
# count stripe edges down the middle column of the rim and of the face.
stripes = pattern(512, 256, lambda x, y: "white" if (y // 4) % 2 else "black")
lens = render(stripes, 512, 256, 0.75)

def stripe_edges(rows):
    levels = [lens.pixelColor(256, y).red() for y in rows]
    midpoint = (min(levels) + max(levels)) / 2
    return sum((a > midpoint) != (b > midpoint) for a, b in zip(levels, levels[1:]))

rim_rows, face_rows = range(2, 40), range(80, 176)
assert stripe_edges(rim_rows) > len(rim_rows) / 4, "Glass rim stretches the background instead of compressing it"
assert stripe_edges(face_rows) < len(face_rows) / 4, "Glass face does not magnify the background"

# The tips of a long pill are antialiased over about one pixel, like its sides.
pill = render(white, 400, 40, 0.75)
row = [pill.pixelColor(x, 20).alpha() for x in range(40)]
assert sum(1 for alpha in row if 8 < alpha < 247) <= 2, f"Pill tip edge is soft: {row}"

# Plasma draws mostly white content on the glass. Over a bright background
# the face darkens so that content stays readable; over a dark background it
# is not darkened.
bright_face = render(white, 256, 128, 0.75).pixelColor(128, 64).red()
assert bright_face <= 200, f"Glass over white stays too bright for white content: {bright_face}"
dark_face = render(pattern(64, 64, lambda x, y: "#404040"), 256, 128, 0.75).pixelColor(128, 64).red()
assert dark_face >= 0x40, f"Glass darkened a dark background: {dark_face}"

assert "OverShifted/LiquidGlass" in glass, "LiquidGlass source attribution is missing"
print("Glass shader checks passed: compressed rim and magnified face, anchored center, crisp tips, valid premultiplied output, readable face over bright backgrounds.")

program.release()
vao.destroy()
