#!/usr/bin/env python3
"""Minimal stdlib-only PNG read/write for the quality gates (no Pillow).

Supports 8-bit RGB (color type 2) and RGBA (color type 6), the formats Aseprite
exports and our fixtures use. Reading handles all five PNG scanline filters so it
can ingest real exports, not only our own filter-0 output.

Pixels are represented as a flat list of (r, g, b, a) tuples, row-major.
"""
import struct
import zlib

PNG_SIG = b"\x89PNG\r\n\x1a\n"


def _paeth(a: int, b: int, c: int) -> int:
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    if pb <= pc:
        return b
    return c


def _unfilter(ftype: int, line: bytearray, prev: bytearray, bpp: int) -> None:
    """Reverse a PNG scanline filter in place (line becomes raw bytes)."""
    n = len(line)
    if ftype == 0:
        return
    for i in range(n):
        x = line[i]
        a = line[i - bpp] if i >= bpp else 0
        b = prev[i]
        c = prev[i - bpp] if i >= bpp else 0
        if ftype == 1:      # Sub
            line[i] = (x + a) & 0xFF
        elif ftype == 2:    # Up
            line[i] = (x + b) & 0xFF
        elif ftype == 3:    # Average
            line[i] = (x + ((a + b) >> 1)) & 0xFF
        elif ftype == 4:    # Paeth
            line[i] = (x + _paeth(a, b, c)) & 0xFF
        else:
            raise ValueError(f"unknown PNG filter type {ftype}")


def read_png(path):
    """Return (width, height, pixels) where pixels is a flat list of (r,g,b,a)."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != PNG_SIG:
        raise ValueError(f"{path}: not a PNG")
    pos = 8
    width = height = bit_depth = color_type = None
    idat = bytearray()
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        ctype = data[pos + 4 : pos + 8]
        chunk = data[pos + 8 : pos + 8 + length]
        pos += 12 + length  # length(4) + type(4) + data + crc(4)
        if ctype == b"IHDR":
            width, height, bit_depth, color_type = struct.unpack(">IIBB", chunk[:10])
        elif ctype == b"IDAT":
            idat += chunk
        elif ctype == b"IEND":
            break
    if bit_depth != 8 or color_type not in (2, 6):
        raise ValueError(
            f"{path}: only 8-bit RGB/RGBA supported (got depth={bit_depth}, color={color_type})"
        )
    channels = 4 if color_type == 6 else 3
    raw = zlib.decompress(bytes(idat))
    stride = width * channels
    prev = bytearray(stride)
    pixels = []
    p = 0
    for _y in range(height):
        ftype = raw[p]
        p += 1
        line = bytearray(raw[p : p + stride])
        p += stride
        _unfilter(ftype, line, prev, channels)
        prev = line
        for x in range(width):
            o = x * channels
            if channels == 4:
                pixels.append((line[o], line[o + 1], line[o + 2], line[o + 3]))
            else:
                pixels.append((line[o], line[o + 1], line[o + 2], 255))
    return width, height, pixels


def write_png(path, width, height, pixels):
    """Write an 8-bit RGBA PNG (filter 0) from a flat list of (r,g,b,a)."""

    def chunk(typ: bytes, payload: bytes) -> bytes:
        return (
            struct.pack(">I", len(payload))
            + typ
            + payload
            + struct.pack(">I", zlib.crc32(typ + payload) & 0xFFFFFFFF)
        )

    raw = bytearray()
    idx = 0
    for _y in range(height):
        raw.append(0)  # filter type: None
        for _x in range(width):
            r, g, b, a = pixels[idx]
            idx += 1
            raw += bytes((r & 0xFF, g & 0xFF, b & 0xFF, a & 0xFF))
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    idat = zlib.compress(bytes(raw), 9)
    with open(path, "wb") as f:
        f.write(PNG_SIG + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b""))
