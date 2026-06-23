"""§C long-session degradation (donut test) scorer.

Renders an N-frame walk-cycle op-plan (16x16, goblin-default palette) and snapshots
the SPEC-007 quality vector {linter pass-rate, min adjacent-frame silhouette-IoU,
off-palette count} at CUMULATIVE context-fill checkpoints (frames 1..k), so a decaying
trend across the long generation is visible. Uses the project's own lint_sprite +
silhouette_iou helpers (same code the Tier-A gate uses).

Usage:  python _donut_score.py <frames.json> <out_base>
  frames.json: {"frames": [ {"ops":[...]}, ... ]}  (ops: rect/line/pixel like _apply_ops)
  -> writes <out_base>_strip_x16.png and <out_base>_snapshots.json; prints the snapshots.
"""
import importlib.util
import json
import os
import sys

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import lint_sprite  # noqa: E402

_spec = importlib.util.spec_from_file_location("sil", os.path.join(ROOT, "tools", "silhouette_iou.py"))
sil = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(sil)

SIZE = 16
PALPATH = os.path.join(ROOT, "knowledge", "palettes", "goblin-default.json")


def hexrgba(h):
    h = h.strip().lstrip("#")
    if len(h) == 8:
        return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4, 6))
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), 255)


def _line(grid, x0, y0, x1, y1, c):
    dx, dy = abs(x1 - x0), -abs(y1 - y0)
    sx, sy = (1 if x0 < x1 else -1), (1 if y0 < y1 else -1)
    err = dx + dy
    while True:
        if 0 <= x0 < SIZE and 0 <= y0 < SIZE:
            grid[y0 * SIZE + x0] = c
        if x0 == x1 and y0 == y1:
            break
        e2 = 2 * err
        if e2 >= dy:
            err += dy; x0 += sx
        if e2 <= dx:
            err += dx; y0 += sy


def render(ops):
    grid = [(0, 0, 0, 0)] * (SIZE * SIZE)
    for op in ops:
        c = hexrgba(op["color"])
        k = op["op"]
        if k == "pixel":
            x, y = int(op["x"]), int(op["y"])
            if 0 <= x < SIZE and 0 <= y < SIZE:
                grid[y * SIZE + x] = c
        elif k == "line":
            _line(grid, int(op["x1"]), int(op["y1"]), int(op["x2"]), int(op["y2"]), c)
        elif k == "rect":
            x1, y1, x2, y2 = int(op["x1"]), int(op["y1"]), int(op["x2"]), int(op["y2"])
            for yy in range(min(y1, y2), max(y1, y2) + 1):
                for xx in range(min(x1, x2), max(x1, x2) + 1):
                    if 0 <= xx < SIZE and 0 <= yy < SIZE:
                        grid[yy * SIZE + xx] = c
    return grid


def strip_of(pxs, k):
    w = SIZE * k
    s = [(0, 0, 0, 0)] * (w * SIZE)
    for f in range(k):
        for y in range(SIZE):
            for x in range(SIZE):
                s[y * w + f * SIZE + x] = pxs[f][y * SIZE + x]
    return w, s


def adj_min_iou(pxs, k):
    if k < 2:
        return 1.0
    w, s = strip_of(pxs, k)
    return sil.series(sil.strip_masks(w, SIZE, s, SIZE))["min"]


def main():
    frames_path, out_base = sys.argv[1], sys.argv[2]
    data = json.load(open(frames_path, encoding="utf-8"))
    frames = data["frames"] if isinstance(data, dict) else data
    pal = lint_sprite.load_palette(PALPATH)

    pxs, per = [], []
    for i, fr in enumerate(frames):
        ops = fr["ops"] if isinstance(fr, dict) else fr
        px = render(ops)
        pxs.append(px)
        findings, counts, _ = lint_sprite.lint(SIZE, SIZE, px, palette=pal)
        per.append({"frame": i + 1, "findings": len(findings),
                    "off_palette": counts.get("off_palette", 0), "orphan": counts.get("orphan", 0)})

    n = len(pxs)
    w, s = strip_of(pxs, n)
    img = Image.new("RGBA", (w, SIZE))
    img.putdata(s)
    img.resize((w * 16, SIZE * 16), Image.NEAREST).save(out_base + "_strip_x16.png")

    # cumulative checkpoints across the long generation (~25/50/75/100% of frames)
    cps = sorted(set([max(2, round(n * f)) for f in (0.25, 0.5, 0.75, 1.0)]))
    snaps = []
    for k in cps:
        clean = sum(1 for p in per[:k] if p["findings"] == 0)
        snaps.append({
            "checkpoint": round(100 * k / n),
            "frames": k,
            "linter": round(clean / k, 3),
            "min_iou": round(adj_min_iou(pxs, k), 3),
            "off_palette": sum(p["off_palette"] for p in per[:k]),
        })
    json.dump({"per_frame": per, "snapshots": snaps}, open(out_base + "_snapshots.json", "w"), indent=2)
    print(json.dumps(snaps, indent=2))


if __name__ == "__main__":
    main()
