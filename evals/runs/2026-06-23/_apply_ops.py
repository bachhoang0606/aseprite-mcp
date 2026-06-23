"""Faithful, deterministic rasterizer for de-confounded Persona A/B (runs 2-3).

Applies an executor agent's exact draw-op plan to a transparent 32x32 canvas with
ZERO creative input: filled rect (inclusive corners), 1px Bresenham line, single
pixel. Off-palette colours are applied VERBATIM (not snapped) and counted, so a
palette-discipline violation is visible to the blind judges rather than hidden.

Usage:  python _apply_ops.py <plan.json> <out_base>
  -> writes <out_base>.png (32x32) and <out_base>_x16.png (512x512 NN upscale)
  -> prints {opaque_px, on_palette_pct, off_palette_colours}
"""
import json
import sys

from PIL import Image

PALETTE = {
    "#1a1626", "#f2c8a0", "#d99a6c", "#a86b48",
    "#6db04a", "#4a8a32", "#2f5e22", "#d8e0ec",
    "#9aa6b8", "#5a6478", "#7a4a2a", "#4a2c18",
}


def hex_to_rgba(h):
    h = h.strip().lstrip("#")
    if len(h) == 8:
        return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), int(h[6:8], 16))
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), 255)


def put(px, x, y, rgba):
    if 0 <= x < 32 and 0 <= y < 32:
        px[x, y] = rgba


def line(px, x0, y0, x1, y1, rgba):
    dx = abs(x1 - x0); dy = -abs(y1 - y0)
    sx = 1 if x0 < x1 else -1
    sy = 1 if y0 < y1 else -1
    err = dx + dy
    while True:
        put(px, x0, y0, rgba)
        if x0 == x1 and y0 == y1:
            break
        e2 = 2 * err
        if e2 >= dy:
            err += dy; x0 += sx
        if e2 <= dx:
            err += dx; y0 += sy


def main():
    plan_path, out_base = sys.argv[1], sys.argv[2]
    with open(plan_path, encoding="utf-8") as f:
        plan = json.load(f)
    ops = plan["ops"] if isinstance(plan, dict) else plan

    img = Image.new("RGBA", (32, 32), (0, 0, 0, 0))
    px = img.load()
    off = {}
    for op in ops:
        c = op["color"].strip().lower()
        if c not in PALETTE:
            off[c] = off.get(c, 0) + 1
        rgba = hex_to_rgba(c)
        kind = op["op"]
        if kind == "pixel":
            put(px, int(op["x"]), int(op["y"]), rgba)
        elif kind == "line":
            line(px, int(op["x1"]), int(op["y1"]), int(op["x2"]), int(op["y2"]), rgba)
        elif kind == "rect":
            x1, y1, x2, y2 = int(op["x1"]), int(op["y1"]), int(op["x2"]), int(op["y2"])
            for yy in range(min(y1, y2), max(y1, y2) + 1):
                for xx in range(min(x1, x2), max(x1, x2) + 1):
                    put(px, xx, yy, rgba)

    img.save(out_base + ".png")
    img.resize((512, 512), Image.NEAREST).save(out_base + "_x16.png")

    opaque = sum(1 for y in range(32) for x in range(32) if px[x, y][3] != 0)
    on_pal = sum(
        1 for y in range(32) for x in range(32)
        if px[x, y][3] != 0 and "#%02x%02x%02x" % px[x, y][:3] in PALETTE
    )
    pct = round(100.0 * on_pal / opaque, 1) if opaque else 0.0
    print(json.dumps({
        "out": out_base + "_x16.png",
        "opaque_px": opaque,
        "on_palette_pct": pct,
        "off_palette_op_colours": off,
    }))


if __name__ == "__main__":
    main()
