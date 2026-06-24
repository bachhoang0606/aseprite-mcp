"""Build descriptions.json: {tool: {full, trimmed}} for the usage A/B.

`full` = the real tool-level description from server.rs (cleaned of `\` line-continuation
artifacts). `trimmed` = a concise rewrite that keeps the params but DROPS the verbose how-to —
including, deliberately, save_preview's coordinate-inversion formula, so the harness can detect
whether dropping load-bearing detail hurts correct usage.
"""
import json
import re

full_raw = json.load(open("evals/tool_usage/_full_desc.json", encoding="utf-8"))


def clean(s):
    s = s.replace("\\ ", " ").replace("\\", " ")     # drop Rust line-continuation backslashes
    s = s.replace("�", "->").replace("→", "->")
    return re.sub(r"\s+", " ", s).strip()


TRIMMED = {
    "live_save_preview":
        "Save an upscaled, vision-legible PNG preview of the active sprite. Options: scale (int), "
        "gutter (coordinate ticks, default on), crop ('sprite'/'cel'/rect), inline (also return the "
        "image), marks_from ('slices'/'layers'/'components').",
    "live_import_reference":
        "Import a PNG reference as palette-locked pixel art on a layer. Params: filename, "
        "width/height (target grid), method ('dominant'/'average'), palette OR auto_colors "
        "(+palette_method), snap, regrid (de-fake scaled art), layer, at_x/at_y.",
    "live_import_animation":
        "Import a generated animation as a palette-locked Aseprite animation. Source: "
        "filename+sheet{cols,rows} OR frames[]. Params: width/height, palette OR auto_colors "
        "(+palette_method), regrid, layer, start_frame, tag, fps, at_x/at_y.",
    "live_rotate":
        "Rotate a region by any angle onto a NEW layer, palette-legal. Params: angle (deg, +cw), "
        "rect OR selection_only, at_x/at_y, layer.",
    "live_dither_fill":
        "Ordered Bayer dither-fill a rectangle between two palette colours. Params: "
        "rect{x,y,width,height}, color_a, color_b, level (0..1 = fraction of color_b), "
        "matrix ('bayer4'/'bayer2'/'checker'), layer.",
    "live_create_autotile_template":
        "Compose an autotile sheet from 4 corner quarters drawn as a strip [fill|outer|edge|inner]. "
        "Params: tile_size (even), layout ('blob47'/'wang16'), source_x/source_y, at_x/at_y, layer.",
}

out = {}
for tool, raw in full_raw.items():
    f = clean(raw)
    t = TRIMMED[tool]
    out[tool] = {"full": f, "trimmed": t, "full_chars": len(f), "trimmed_chars": len(t)}
json.dump(out, open("evals/tool_usage/descriptions.json", "w", encoding="utf-8"), indent=2, ensure_ascii=False)
for tool, d in out.items():
    print(f"{tool}: full {d['full_chars']} -> trimmed {d['trimmed_chars']} chars")
