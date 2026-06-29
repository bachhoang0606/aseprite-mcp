"""Assemble the aseprite-mcp demo reel from REAL project cores + REAL session artifacts.

Every animated pixel here is produced by the same op-rasterizer/palette the tool uses
(`pixels.py`), and every "receipt" is a real PNG emitted in a live session under
`evals/runs/`. Captions are a hand-rolled bitmap font (`font.py`). Nearest-neighbor
scaling only — no blur, whole-pixel aligned. End-card carries "Real tool outputs.
Staged for clarity."

Usage:
    python demo/reel/reel.py [--cut 45|30] [--fps 12] [--canvas 480] [--frames]
Outputs (demo/reel/out/):
    reel_<cut>s.gif   the looping hero (README + social)
    poster.png        a representative still
    frames/*.png      (only with --frames) the master sequence → MP4 later via ffmpeg
"""
import argparse
import math
import os
import sys

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)
import font  # noqa: E402
import pixels  # noqa: E402

# ---- palette / theme ----
BG = (20, 18, 30, 255)
PANEL_L = (40, 37, 58, 255)
PANEL_D = (31, 29, 47, 255)
FRAME = (58, 54, 80, 255)
INK = (232, 224, 208, 255)
DIM = (150, 145, 168, 255)
GREEN = (106, 190, 48, 255)
RED = (216, 32, 46, 255)
GOLD = (214, 178, 74, 255)
CHIP_BG = (33, 30, 47, 255)
BACK = (12, 11, 20, 210)  # caption backing

ASSETS = {
    "goblin_before": os.path.join(ROOT, "evals/runs/2026-06-22/cand_A.png"),
    "goblin_after": os.path.join(ROOT, "evals/runs/2026-06-22/cand_B.png"),
    "knight": os.path.join(ROOT, "evals/runs/2026-06-24/v_anim_idle.png"),
    "autotile": os.path.join(ROOT, "evals/runs/2026-06-24/v_autotile.png"),
    "dither": os.path.join(ROOT, "evals/runs/2026-06-24/v_dither_gradient.png"),
    "rotate": os.path.join(ROOT, "evals/runs/2026-06-24/v_rotate.png"),
}

CFG = {}  # filled by main(): CANVAS, FPS, PANEL_CY


# ---------- low-level helpers ----------
def base():
    return Image.new("RGBA", (CFG["CANVAS"], CFG["CANVAS"]), BG)


def up(img, scale):
    return img.resize((img.width * scale, img.height * scale), Image.NEAREST)


def rect(d, x, y, w, h, c):
    d.alpha_composite(Image.new("RGBA", (max(1, int(w)), max(1, int(h)), ), c), (int(x), int(y)))


def hollow_rect(canvas, x, y, w, h, c, t=2):
    rect(canvas, x, y, w, t, c)
    rect(canvas, x, y + h - t, w, t, c)
    rect(canvas, x, y, t, h, c)
    rect(canvas, x + w - t, y, t, h, c)


def checker_panel(w, h, sq=12):
    img = Image.new("RGBA", (w, h), PANEL_D)
    d = img.load()
    for yy in range(h):
        for xx in range(w):
            if ((xx // sq) + (yy // sq)) % 2 == 0:
                d[xx, yy] = PANEL_L
    return img


def panel_rect(S, pad=3):
    """Panel box (x, y, w, h) sized to wrap a 24px hero at scale S with `pad` hero-px margin."""
    side = (24 + 2 * pad) * S
    x = (CFG["CANVAS"] - side) // 2
    y = CFG["PANEL_CY"] - side // 2
    return x, y, side, side


def hero_origin(S, pad=3):
    x, y, side, _ = panel_rect(S, pad)
    return x + pad * S, y + pad * S


def draw_panel(canvas, S, pad=3):
    x, y, w, h = panel_rect(S, pad)
    hollow_rect(canvas, x - 3, y - 3, w + 6, h + 6, FRAME, t=3)
    canvas.alpha_composite(checker_panel(w, h, sq=max(6, S)), (x, y))
    return x, y, w, h


def reveal_down(img, frac):
    """Show only the top `frac` of img's non-transparent rows (a 'being drawn' wipe)."""
    if frac >= 1.0:
        return img
    out = img.copy()
    cut = int(img.height * frac)
    if cut < img.height:
        clear = Image.new("RGBA", (img.width, img.height - cut), (0, 0, 0, 0))
        out.paste(clear, (0, cut))
    return out


def hero_layer(ops_or_stage, S):
    img = pixels.render(ops_or_stage if isinstance(ops_or_stage, list) else pixels.stage(ops_or_stage))
    return up(img, S)


def place_hero(canvas, img, S, pad=3, shadow=True):
    hx, hy = hero_origin(S, pad)
    if shadow:
        sh = Image.new("RGBA", (16 * S, max(2, S)), (0, 0, 0, 90))
        canvas.alpha_composite(sh, (hx + 4 * S, hy + 24 * S))
    canvas.alpha_composite(img, (hx, hy))
    return hx, hy


def fit_nearest(im, bw, bh):
    w, h = im.size
    scale = min(bw / w, bh / h)
    if scale >= 1:
        scale = math.floor(scale)
        ns = (w * scale, h * scale)
    else:
        ns = (max(1, round(w * scale)), max(1, round(h * scale)))
    return im.resize(ns, Image.NEAREST)


_RCACHE = {}


def receipt(name, bw, bh):
    key = (name, bw, bh)
    if key not in _RCACHE:
        im = Image.open(ASSETS[name]).convert("RGBA")
        bb = im.getbbox()
        if bb:
            im = im.crop(bb)
        _RCACHE[key] = fit_nearest(im, bw, bh)
    return _RCACHE[key]


def paste_center(canvas, img, cx, cy):
    canvas.alpha_composite(img, (int(cx - img.width / 2), int(cy - img.height / 2)))


# ---------- UI bits ----------
def caption(canvas, lines, scale, cy, fg=INK, back=True):
    if isinstance(lines, str):
        lines = [lines]
    lh = font.GH * scale + 6
    total = lh * len(lines)
    if back:
        maxw = max(font.measure(s, scale)[0] for s in lines)
        rect(canvas, (CFG["CANVAS"] - maxw) // 2 - 12, cy - 6, maxw + 24, total + 6, BACK)
    for i, s in enumerate(lines):
        font.draw_text(canvas, s, scale, CFG["CANVAS"] / 2, cy + i * lh, fg)


def chip(canvas, text, x, y, scale=2, fg=INK, bg=CHIP_BG, anchor="l"):
    tw, th = font.measure(text, scale)
    padx, pady = 8, 6
    w, h = tw + 2 * padx, th + 2 * pady
    if anchor == "r":
        x = x - w
    elif anchor == "c":
        x = x - w // 2
    rect(canvas, x, y, w, h, bg)
    rect(canvas, x, y, w, 2, (FRAME[0], FRAME[1], FRAME[2], 255))
    t = font.text_image(text, scale, fg)
    canvas.alpha_composite(t, (int(x + padx), int(y + pady)))
    return w, h


def live_badge(canvas, blink_on=True):
    x, y = 16, 16
    if blink_on:
        rect(canvas, x, y + 3, 12, 12, RED)
    font.draw_text(canvas, "LIVE", 2, x + 6 + 30, y + 4, INK)


def hud(canvas, palette_on, stat=None):
    pr = panel_rect(11)
    pb = pr[1] + pr[3]  # panel bottom; keep both chips below it, on separate rows
    if stat:
        chip(canvas, stat, CFG["CANVAS"] - 14, pb + 4, 2, GOLD, CHIP_BG, "r")
    if palette_on:
        chip(canvas, "OFF-PALETTE: 0 ✓", 14, pb + 38, 2, GREEN, CHIP_BG, "l")


def palette_dock(canvas, n_shown):
    """Vertical swatch dock on the right (Aseprite-style), avoids the caption band."""
    cols = pixels.PALETTE_HEX
    n = min(n_shown, len(cols))
    sw, gap = 16, 4
    total = len(cols) * (sw + gap) - gap
    pr = panel_rect(11)
    x = pr[0] + pr[2] + 12
    y0 = CFG["PANEL_CY"] - total // 2
    for i, hx in enumerate(cols[:n]):
        y = y0 + i * (sw + gap)
        rect(canvas, x, y, sw, sw, pixels._hx(hx))
        hollow_rect(canvas, x, y, sw, sw, (0, 0, 0, 160), t=1)


# ---------- beats ----------
def b_cold_open(i, n):
    c = base()
    p = i / max(1, n - 1)
    S = 11
    draw_panel(c, S)
    sil = hero_layer("silhouette", S)
    c2 = c
    place_hero(c2, reveal_down(sil, min(1.0, p * 1.7)), S)
    live_badge(c2, blink_on=(i // 4) % 2 == 0)
    if p < 0.5:
        caption(c2, ["I GAVE AN AI MY", "OPEN ASEPRITE FILE."], 3, 34)
    else:
        caption(c2, "IT STARTED DRAWING.", 3, 40)
    return c2


def b_palette_flats(i, n):
    c = base()
    p = i / max(1, n - 1)
    S = 11
    draw_panel(c, S)
    nshow = min(12, 1 + int(p * 18))
    palette_dock(c, nshow)
    if p < 0.45:
        place_hero(c, hero_layer("silhouette", S), S)
    else:
        fr = min(1.0, (p - 0.45) / 0.35)
        place_hero(c, reveal_down(hero_layer("flats", S), fr), S)
    live_badge(c, (i // 4) % 2 == 0)
    caption(c, ["LOCKS A PALETTE.", "BLOCKS THE SHAPE."], 3, 30)
    hud(c, palette_on=(p > 0.45))
    return c


def b_shading(i, n):
    c = base()
    p = i / max(1, n - 1)
    S = 11
    draw_panel(c, S)
    palette_dock(c, 12)
    # body shades in over the flats
    if p < 0.2:
        place_hero(c, hero_layer("flats", S), S)
    else:
        fr = min(1.0, (p - 0.2) / 0.35)
        # composite: flats fully, shading wipes down
        base_flats = hero_layer("flats", S)
        shaded = hero_layer("shaded", S)
        merged = base_flats.copy()
        merged.alpha_composite(reveal_down(shaded, fr))
        place_hero(c, merged, S)
    live_badge(c, (i // 4) % 2 == 0)
    caption(c, ["HUE-SHIFTED SHADING —", "EVERY PIXEL PALETTE-LEGAL."], 3, 28)
    hud(c, palette_on=True, stat="CONSTRAINED COLOR +75PP")
    # cutaway thumbnails (dither / rotate) flashing in the left gutter (clear of caption)
    if p > 0.55:
        font.draw_text(c, "REAL OUTPUT", 1, 37, CFG["PANEL_CY"] - 90, DIM)
        for k, name in enumerate(["dither", "rotate"]):
            th = receipt(name, 62, 62)
            x, y = 6, CFG["PANEL_CY"] - 74 + k * 74
            c.alpha_composite(th, (x, y))
            hollow_rect(c, x - 2, y - 2, th.width + 4, th.height + 4, GREEN, t=2)
    return c


def b_self_critique(i, n):
    c = base()
    p = i / max(1, n - 1)
    S = 11
    draw_panel(c, S)
    hx, hy = hero_origin(S)
    ring_blink = (i // 2) % 2 == 0
    if p < 0.30:
        place_hero(c, hero_layer("flawed", S), S)
        if ring_blink:
            hollow_rect(c, hx + 13 * S, hy + 8 * S, 4 * S, 4 * S, RED, t=3)
        caption(c, "THEN IT LOOKS AT ITS OWN WORK…", 2, 34)
    elif p < 0.52:
        place_hero(c, hero_layer("flawed", S), S)
        if (i // 1) % 2 == 0:
            hollow_rect(c, hx + 13 * S, hy + 8 * S, 4 * S, 4 * S, RED, t=3)
        caption(c, "WAIT — THE EYES DON'T MATCH", 3, 32, fg=RED)
    elif p < 0.66:
        place_hero(c, hero_layer("fixed", S), S)
        font.draw_text(c, "✓", 5, hx + 22 * S, hy + 2 * S, GREEN)
        caption(c, "…AND FIXES IT.", 4, 38, fg=GREEN)
    else:
        # real receipt: before / after
        bw = (CFG["CANVAS"] - 60) // 2
        before = receipt("goblin_before", bw, 220)
        after = receipt("goblin_after", bw, 220)
        cy = CFG["PANEL_CY"] - 6
        paste_center(c, before, CFG["CANVAS"] * 0.28, cy)
        paste_center(c, after, CFG["CANVAS"] * 0.72, cy)
        font.draw_text(c, "BEFORE", 2, CFG["CANVAS"] * 0.28, cy + 118, DIM)
        font.draw_text(c, "AFTER", 2, CFG["CANVAS"] * 0.72, cy + 118, GREEN)
        caption(c, "REAL OUTPUT — IT FIXED ITS OWN SHADING", 2, 34)
    live_badge(c, (i // 4) % 2 == 0)
    hud(c, palette_on=True, stat="PERCEPTION +1.33 BLIND-JUDGED")
    return c


def b_walk(i, n):
    c = base()
    p = i / max(1, n - 1)
    S = 11
    draw_panel(c, S)
    fr = pixels.walk_frame((i // 2) % 4)
    place_hero(c, up(pixels.render(fr), S), S)
    live_badge(c, (i // 4) % 2 == 0)
    caption(c, "4-FRAME WALK CYCLE.", 3, 36)
    hud(c, palette_on=True)
    # knight filmstrip flash (real output) — top-right pip, clear of hero + caption
    if p > 0.5:
        k = receipt("knight", 110, 110)
        x, y = CFG["CANVAS"] - 8 - k.width, 74
        c.alpha_composite(k, (x, y))
        hollow_rect(c, x - 2, y - 2, k.width + 4, k.height + 4, GREEN, t=2)
        font.draw_text(c, "REAL OUTPUT ↑", 1, x + k.width / 2, y - 12, DIM)
    return c


def b_autotile(i, n):
    c = base()
    p = i / max(1, n - 1)
    S = 11
    draw_panel(c, S)
    tile = receipt("autotile", 250, 250)
    paste_center(c, tile, CFG["CANVAS"] / 2, CFG["PANEL_CY"])
    # hero standing on the autotiled ground
    hero = up(pixels.render(pixels.walk_frame(0)), 7)
    paste_center(c, hero, CFG["CANVAS"] / 2, CFG["PANEL_CY"] + 30)
    live_badge(c, (i // 4) % 2 == 0)
    if p < 0.55:
        caption(c, "AUTOTILES A SCENE.", 3, 34)
        hud(c, palette_on=True)
    else:
        caption(c, ["EXPORTS A SHEET.", "ENGINE-READY."], 3, 28)
        hud(c, palette_on=True, stat="→ UNITY · GODOT · TILED")
    return c


def b_endcard(i, n):
    c = base()
    p = i / max(1, n - 1)
    # receipts montage strip
    names = ["goblin_after", "knight", "autotile"]
    th_w = (CFG["CANVAS"] - 80) // 3
    xs = [40 + k * (th_w + 10) + th_w / 2 for k in range(3)]
    for k, nm in enumerate(names):
        t = receipt(nm, th_w, 120)
        paste_center(c, t, xs[k], 120)
        hollow_rect(c, xs[k] - t.width / 2 - 2, 120 - t.height / 2 - 2, t.width + 4, t.height + 4, FRAME, t=2)
    cx = CFG["CANVAS"] / 2
    caption(c, ["LIVE IN YOUR ASEPRITE.", "MEASURED, NOT VIBES."], 3, 200, back=False)
    font.draw_text(c, "GITHUB.COM/BACHHOANG0606/ASEPRITE-MCP", 2, cx, 264, GREEN)
    font.draw_text(c, "V0.2.0 · MIT", 2, cx, 288, DIM)
    font.draw_text(c, "+1.33 PERCEPTION · 0 OFF-PALETTE", 2, cx, 326, INK)
    font.draw_text(c, "+4.0 REFERENCE · BLIND-JUDGED", 2, cx, 350, INK)
    font.draw_text(c, "REAL TOOL OUTPUTS · STAGED FOR CLARITY.", 2, cx, CFG["CANVAS"] - 32, DIM)
    return c


# beat table: (name, fn, dur45, dur30)
BEATS = [
    ("cold_open", b_cold_open, 2.5, 2.0),
    ("palette_flats", b_palette_flats, 6.5, 4.0),
    ("shading", b_shading, 7.0, 4.5),
    ("self_critique", b_self_critique, 10.0, 8.0),
    ("walk", b_walk, 8.0, 5.0),
    ("autotile", b_autotile, 7.0, 4.5),
    ("endcard", b_endcard, 4.0, 2.0),
]


# ---------- GIF assembly ----------
def build_palette(frames):
    """One stable adaptive palette for the whole reel (avoids per-frame flicker)."""
    k = max(1, len(frames) // 8)
    samples = frames[::k]
    strip = Image.new("RGB", (frames[0].width, frames[0].height * len(samples)))
    for idx, f in enumerate(samples):
        strip.paste(f.convert("RGB"), (0, idx * frames[0].height))
    return strip.quantize(colors=128, method=Image.MEDIANCUT, dither=Image.Dither.NONE)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cut", type=int, default=45, choices=[45, 30])
    ap.add_argument("--fps", type=int, default=12)
    ap.add_argument("--canvas", type=int, default=480)
    ap.add_argument("--frames", action="store_true", help="also write PNG frames (MP4 source)")
    a = ap.parse_args()
    CFG["CANVAS"] = a.canvas
    CFG["FPS"] = a.fps
    CFG["PANEL_CY"] = int(a.canvas * 0.50)
    global CANVAS, FPS
    CANVAS, FPS = a.canvas, a.fps

    frames = []
    durs = []
    for name, fn, d45, d30 in BEATS:
        dur = d45 if a.cut == 45 else d30
        nf = max(1, round(dur * a.fps))
        for i in range(nf):
            frames.append(fn(i, nf))
        durs.append((name, nf))
    total_s = len(frames) / a.fps
    print(f"cut={a.cut}s fps={a.fps} canvas={a.canvas} frames={len(frames)} total={total_s:.1f}s")
    for nm, nf in durs:
        print(f"  {nm:16s} {nf:3d}f {nf / a.fps:4.1f}s")

    outdir = os.path.join(HERE, "out")
    os.makedirs(outdir, exist_ok=True)
    rgb = [f.convert("RGB") for f in frames]
    pal = build_palette(rgb)
    pframes = [f.quantize(palette=pal, dither=Image.Dither.NONE) for f in rgb]

    gif_path = os.path.join(outdir, f"reel_{a.cut}s.gif")
    pframes[0].save(
        gif_path, save_all=True, append_images=pframes[1:],
        duration=int(1000 / a.fps), loop=0, optimize=True, disposal=2,
    )
    size_mb = os.path.getsize(gif_path) / 1e6
    print(f"wrote {gif_path}  ({size_mb:.2f} MB)")

    # poster: the 'fixed' moment of the self-critique beat
    poster_idx = sum(nf for nm, nf in durs[:3]) + int(durs[3][1] * 0.60)
    frames[min(poster_idx, len(frames) - 1)].convert("RGB").save(os.path.join(outdir, "poster.png"))

    if a.frames:
        fdir = os.path.join(outdir, "frames")
        os.makedirs(fdir, exist_ok=True)
        for idx, f in enumerate(frames):
            f.convert("RGB").save(os.path.join(fdir, f"f{idx:04d}.png"))
        print(f"wrote {len(frames)} frames -> {fdir}  (gitignored; ffmpeg -> MP4 later)")


if __name__ == "__main__":
    main()
