#!/usr/bin/env python3
"""Build the 40 px XCursor files from the editable SVG groups in artwork.svg."""

import argparse
import pathlib
import struct
import subprocess
import sys
import xml.etree.ElementTree as ET
import zlib


ROOT = pathlib.Path(__file__).resolve().parent
SVG = "http://www.w3.org/2000/svg"
GROUPS = {group.get("id"): group for group in ET.parse(ROOT / "artwork.svg").getroot()}
IMAGE = 0xFFFD0002
OUTPUT = ROOT / "cursors"


def png_rgba(png):
    """Decode resvg's 40 by 40, 8-bit RGBA, non-interlaced PNG without external tools."""
    if png[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("renderer did not return a PNG image")
    position, header, data = 8, None, b""
    while position < len(png):
        length, kind = struct.unpack(">I4s", png[position:position + 8])
        body = png[position + 8:position + 8 + length]
        position += length + 12
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            data += body
    if header != (40, 40, 8, 6, 0, 0, 0):
        raise ValueError("renderer did not return a 40 by 40 RGBA image")
    raw = zlib.decompress(data)
    stride = 40 * 4
    previous = bytearray(stride)
    rows = []
    for y in range(40):
        start = y * (stride + 1)
        kind, line = raw[start], bytearray(raw[start + 1:start + 1 + stride])
        for i in range(stride):
            left = line[i - 4] if i >= 4 else 0
            up = previous[i]
            corner = previous[i - 4] if i >= 4 else 0
            if kind == 1:
                line[i] = (line[i] + left) & 255
            elif kind == 2:
                line[i] = (line[i] + up) & 255
            elif kind == 3:
                line[i] = (line[i] + (left + up) // 2) & 255
            elif kind == 4:
                estimate = left + up - corner
                distances = (abs(estimate - left), abs(estimate - up), abs(estimate - corner))
                nearest = left if distances[0] <= distances[1] and distances[0] <= distances[2] else up if distances[1] <= distances[2] else corner
                line[i] = (line[i] + nearest) & 255
            elif kind:
                raise ValueError(f"unsupported PNG filter {kind}")
        rows.append(bytes(line))
        previous = line
    return b"".join(rows)


def render(parts):
    normal = any(name == "normal" for name, _, _ in parts)
    root = ET.Element(f"{{{SVG}}}svg", {"width": "40", "height": "40", "viewBox": "0 0 40 40"})
    for name, transform, opacity in parts:
        if name == "normal":
            if transform or opacity != 1:
                raise ValueError("normal cross cannot be transformed before its center is replaced")
            name = "precision"
        wrapper = ET.SubElement(root, f"{{{SVG}}}g")
        if transform:
            wrapper.set("transform", transform)
        if opacity != 1:
            wrapper.set("opacity", str(opacity))
        # Copy through serialization so the source tree is never modified.
        wrapper.append(ET.fromstring(ET.tostring(GROUPS[name])))
    svg = ET.tostring(root)
    png = subprocess.run(["resvg", "--resources-dir", str(ROOT), "-w", "40", "-h", "40", "-", "-c"],
                         input=svg, stdout=subprocess.PIPE, check=True).stdout
    rgba = png_rgba(png)
    # XCursor stores straight-alpha ARGB32 pixels in little-endian byte order.
    pixels = bytearray(channel for i in range(0, len(rgba), 4) for channel in (rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]))
    if normal:
        # Replace only the center square; keep every other pixel identical
        # to the precision cursor, including the contour and its shadow.
        center = render([("normal-center", "", 1)])
        for y in range(17, 23):
            for x in range(17, 23):
                index = (y * 40 + x) * 4
                pixels[index:index + 4] = center[index:index + 4]
    return bytes(pixels)


def write_cursor(name, frames):
    toc = []
    chunks = []
    offset = 16 + 12 * len(frames)
    for parts, hot_x, hot_y, delay in frames:
        pixels = render(parts)
        chunk = struct.pack("<9I", 36, IMAGE, 40, 1, 40, 40, hot_x, hot_y, delay) + pixels
        toc.append(struct.pack("<3I", IMAGE, 40, offset))
        chunks.append(chunk)
        offset += len(chunk)
    data = struct.pack("<4I", 0x72756358, 16, 1, len(frames)) + b"".join(toc + chunks)
    (OUTPUT / name).write_bytes(data)


def frame(parts, hotspot=(20, 20), delay=50):
    return (parts, *hotspot, delay)


def static(name, parts, hotspot=(20, 20)):
    write_cursor(name, [frame(parts, hotspot)])


NORMAL = [("normal", "", 1)]
PRECISION = [("precision", "", 1)]


def main():
    static("default", NORMAL)
    static("crosshair", PRECISION)
    static("help", NORMAL + [("help", "", 1)])
    static("no-drop", [("forbidden", "", 1)])
    static("up-arrow", [("up-arrow", "", 1)], (20, 2))
    static("person", [("arrow", "", 1), ("person", "", 1)], (2, 2))
    static("location", [("arrow", "", 1), ("location", "", 1)], (2, 2))
    # Each resize cursor keeps the same cross and adds opposite arrowheads.
    # NW/SE is the forward diagonal; NE/SW is the backward diagonal.
    for name, angle in (("size_hor", 0), ("size_ver", 90), ("size_fdiag", 45), ("size_bdiag", -45)):
        write_cursor(name, [
            frame(NORMAL + [
                ("resize-arrow", f"rotate({direction} 20 20) translate(20 20) scale({scale}) translate(-20 -20)", 1)
                for direction in (angle, angle + 180)
            ], delay=167)
            for scale in (1, 1.12)
        ])
    write_cursor("progress", [frame([("spinner", f"rotate({step * 360 / 25} 20 20)", 0.45 + 0.55 * abs(12 - step) / 12)]) for step in range(25)])
    write_cursor("wait", [frame([("precision", f"rotate({step * 360 / 20} 20 20) scale({scale}) translate({shift} {shift})", 1)])
                          for step in range(20) for scale, shift in [((1, 0) if step % 5 == 0 else (0.78, 5.64))]])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=pathlib.Path, default=OUTPUT,
                        help="directory for generated XCursor files (default: cursors/)")
    OUTPUT = parser.parse_args().output_dir
    try:
        OUTPUT.mkdir(parents=True, exist_ok=True)
        main()
    except (OSError, subprocess.CalledProcessError, ValueError) as error:
        sys.exit(f"Cursor build failed: {error}")
