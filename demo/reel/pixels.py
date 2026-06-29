"""Hero sprite for the demo reel — defined as op-plans on a locked 12-colour palette and
rasterized with the same op model the project uses (rect inclusive / Bresenham line / pixel).
Stages: silhouette -> flats -> shaded -> flawed (mismatched eyes) -> fixed -> walk frames.
Nearest-neighbor only. Pure stdlib + PIL.
"""
from PIL import Image

W = H = 24

# Locked 12-colour goblin palette (hue-shifted ramps; matches the project's goblin-default spirit).
PAL = {
    "O": "#1B1226",  # outline (cool near-black)
    "k0": "#1B4D3E", "k1": "#2E7D32", "k2": "#4CA02C", "k3": "#6ABE30", "k4": "#A6D94A",  # skin ramp
    "l0": "#3A2417", "l1": "#5A3A22", "l2": "#8A5A2B", "l3": "#B07A3C",  # leather ramp
    "w": "#E8E0D0",  # tooth / eye-white
    "r": "#D8202E",  # accent (eye glint / sash)
}
PALETTE_HEX = list(dict.fromkeys(PAL.values()))  # the locked list (for the "off-palette: 0" claim)


def _hx(c):
    c = c.lstrip("#")
    return (int(c[0:2], 16), int(c[2:4], 16), int(c[4:6], 16), 255)


def _line(px, x0, y0, x1, y1, c):
    dx, dy = abs(x1 - x0), -abs(y1 - y0)
    sx, sy = (1 if x0 < x1 else -1), (1 if y0 < y1 else -1)
    e = dx + dy
    while True:
        if 0 <= x0 < W and 0 <= y0 < H:
            px[x0, y0] = c
        if x0 == x1 and y0 == y1:
            break
        e2 = 2 * e
        if e2 >= dy:
            e += dy; x0 += sx
        if e2 <= dx:
            e += dx; y0 += sy


def render(ops):
    """Rasterize an op list to a 24x24 RGBA image (transparent bg)."""
    img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    px = img.load()
    for op in ops:
        c = _hx(PAL[op["c"]]) if op["c"] in PAL else _hx(op["c"])
        k = op["op"]
        if k == "p":
            x, y = op["x"], op["y"]
            if 0 <= x < W and 0 <= y < H:
                px[x, y] = c
        elif k == "ln":
            _line(px, op["x1"], op["y1"], op["x2"], op["y2"], c)
        elif k == "r":
            for yy in range(min(op["y1"], op["y2"]), max(op["y1"], op["y2"]) + 1):
                for xx in range(min(op["x1"], op["x2"]), max(op["x1"], op["x2"]) + 1):
                    if 0 <= xx < W and 0 <= yy < H:
                        px[xx, yy] = c
    return img


def R(x1, y1, x2, y2, c):
    return {"op": "r", "x1": x1, "y1": y1, "x2": x2, "y2": y2, "c": c}


def L(x1, y1, x2, y2, c):
    return {"op": "ln", "x1": x1, "y1": y1, "x2": x2, "y2": y2, "c": c}


def P(x, y, c):
    return {"op": "p", "x": x, "y": y, "c": c}


# ---- the goblin hero, in layered op groups so stages can compose ----
def _body_flat():
    """Flat single-tone regions (the 'blocks the shape' stage)."""
    return [
        R(7, 3, 16, 11, "k1"),     # head
        R(4, 5, 5, 8, "k1"), R(18, 5, 19, 8, "k1"),  # ears
        R(9, 13, 14, 18, "l1"),    # tunic
        R(6, 13, 7, 17, "k1"), R(16, 13, 17, 17, "k1"),  # arms
        R(9, 19, 10, 22, "l1"), R(13, 19, 14, 22, "l1"),  # legs
    ]


def _shading():
    """Hue-shifted ramp shading (light upper-left)."""
    return [
        R(7, 3, 10, 5, "k3"), R(7, 3, 9, 3, "k4"),     # head highlight
        R(13, 9, 16, 11, "k0"),                         # head shadow
        R(9, 13, 11, 14, "l2"), R(13, 16, 14, 18, "l0"),  # tunic light/shadow
        L(9, 16, 14, 16, "O"),                          # belt
        L(6, 13, 6, 17, "k3"), L(17, 13, 17, 17, "k0"),  # arm light/shadow
        P(9, 19, "l2"), P(14, 22, "l0"),
    ]


def _face(fixed=True):
    """Eyes/brow/mouth/nose. When fixed=False, the right eye sits a row low + flatter (the flaw)."""
    o = [
        L(7, 6, 9, 6, "O"), L(14, 6, 16, 6, "O"),       # brows
        R(11, 7, 12, 9, "k0"),                          # nose
        L(9, 10, 14, 10, "O"), P(11, 11, "w"),          # mouth + tooth
        # left eye (always correct)
        P(8, 7, "w"), P(9, 7, "w"), P(8, 8, "O"),
    ]
    if fixed:
        o += [P(14, 7, "w"), P(15, 7, "w"), P(15, 8, "O"), P(8, 7, "r"), P(15, 7, "r")]  # symmetric + glints
    else:
        o += [P(14, 9, "w"), P(15, 9, "w"), P(15, 10, "O")]  # right eye 2 rows LOW, lopsided -> obvious flaw
    return o


def _outline():
    """1px dark silhouette outline ring (drawn under fills via explicit edge pixels)."""
    o = []
    edges = [(7, 2, 16, 2), (6, 12, 6, 12), (3, 5, 3, 8), (20, 5, 20, 8),
             (8, 19, 8, 22), (15, 19, 15, 22), (8, 23, 16, 23)]
    for x1, y1, x2, y2 in edges:
        o.append(L(x1, y1, x2, y2, "O"))
    o += [L(6, 3, 6, 11, "O"), L(17, 3, 17, 11, "O"), L(8, 13, 8, 18, "O"), L(15, 13, 15, 18, "O")]
    return o


def stage(name):
    if name == "silhouette":
        return _outline() + [R(7, 3, 16, 11, "k0"), R(9, 13, 14, 18, "k0"),
                             R(6, 13, 7, 17, "k0"), R(16, 13, 17, 17, "k0"),
                             R(9, 19, 10, 22, "k0"), R(13, 19, 14, 22, "k0"),
                             R(4, 5, 5, 8, "k0"), R(18, 5, 19, 8, "k0")]
    if name == "flats":
        return _outline() + _body_flat()
    if name == "shaded":
        return _outline() + _body_flat() + _shading()  # body shaded, face not drawn yet
    if name == "flawed":
        return _outline() + _body_flat() + _shading() + _face(fixed=False)
    if name == "fixed":
        return _outline() + _body_flat() + _shading() + _face(fixed=True)
    raise ValueError(name)


def walk_frame(i):
    """4-frame walk: legs alternate, 1px body bob (i in 0..3)."""
    bob = [0, -1, 0, -1][i]
    legs = [
        [R(9, 19, 10, 22 + bob, "l1"), R(13, 19, 14, 21, "l1")],
        [R(9, 19, 10, 21, "l1"), R(13, 19, 14, 22, "l1")],
        [R(9, 19, 10, 22, "l1"), R(13, 19, 14, 21, "l1")],
        [R(9, 19, 10, 21, "l1"), R(13, 19, 14, 22, "l1")],
    ][i]
    base = [op for op in (_outline() + _body_flat() + _shading() + _face(True))
            if not (op.get("op") == "r" and op.get("y1") == 19)]  # drop default legs
    # shift the upper body by bob
    shifted = []
    for op in base:
        op = dict(op)
        for ky in ("y1", "y2", "y"):
            if ky in op and op[ky] <= 18:
                op[ky] += bob
        shifted.append(op)
    return shifted + legs


if __name__ == "__main__":
    import os
    os.makedirs("demo/reel/out", exist_ok=True)
    montage = Image.new("RGBA", (W * 6, H), (40, 40, 48, 255))
    for i, nm in enumerate(["silhouette", "flats", "flawed", "fixed"]):
        montage.paste(render(stage(nm)), (i * W, 0))
    for i in range(2):
        montage.paste(render(walk_frame(i)), ((4 + i) * W, 0))
    montage.resize((W * 6 * 12, H * 12), Image.NEAREST).save("demo/reel/out/_hero_stages.png")
    print("palette:", len(PALETTE_HEX), "colours; wrote _hero_stages.png")
