"""Apply the measured-safe description trims to src/server.rs (one-off).

Replaces the `description = "..."` of each candidate tool with a concise version that keeps the
essential cue (incl. save_preview's one-line gutter-inversion rule) and drops the verbose prose.
Idempotent-ish: matches the current string literal before `async fn <tool>` and replaces it.
"""
import re

NEW = {
    "live_save_preview":
        "Save an upscaled, vision-legible PNG preview of the active sprite (nearest-neighbor, "
        "auto-scaled so the long edge nears ~1024px; raw 1x of small sprites is unreadable to vision "
        "models). Use this whenever the agent needs to SEE its work. Options: `scale` (int, override "
        "the auto factor, <=16x); `gutter` (labelled coordinate ticks, default on -- to map a preview "
        "pixel back to source: source = (preview - gutter_band) / scale, or preview/scale when off; "
        "see `gutter_applied`/`gutter_skipped`); `crop` (\\\"sprite\\\" default / \\\"cel\\\" / "
        "{x,y,width,height}; crop origin is reported and baked into the gutter labels); `inline:true` "
        "(also return the PNG as a base64 image block; oversized degrades to path + note); "
        "`marks_from` (\\\"slices\\\"/\\\"layers\\\"/\\\"components\\\" -> numbered Set-of-Mark badges "
        "+ a `marks` [{n,region,bbox}] map). Returns source/preview size, scale, crop origin, gutter "
        "extents.",
    "live_import_reference":
        "SPEC-006: import a PNG reference as palette-locked pixel art on a layer -- the unlock for the "
        "reference/trace pipeline. Content-aware downscales to a target grid then snaps to a palette. "
        "Params: filename (PNG); width/height (target, default sprite size); method "
        "(\\\"dominant\\\" majority/default or \\\"average\\\"); palette (#rrggbb list) OR auto_colors:N "
        "(+palette_method median_cut/kmeans/frequency) OR snap:false (keep source colours); regrid "
        "(de-fake a scaled reference to its native grid); layer (default \\\"Reference\\\"), at_x/at_y. "
        "Target capped 256px/edge.",
    "live_import_animation":
        "SPEC-012 (free Path-3): import a user-supplied generated ANIMATION as a palette-locked "
        "Aseprite animation. Source: EITHER filename + sheet{cols,rows} (a sprite-sheet PNG sliced "
        "row-major) OR frames[] (PNG paths, uniform size). Each frame is downscaled + snapped to ONE "
        "shared palette across all frames (palette OR auto_colors:N +palette_method) so it doesn't "
        "colour-flicker; regrid de-fakes scaled frames. Params: width/height, method, layer (default "
        "\\\"Reference Anim\\\"), start_frame, tag (default \\\"ref\\\"), fps (default 12), at_x/at_y. "
        "PNG only, <=64 frames.",
    "live_rotate":
        "SPEC-009: artifact-free RotSprite rotation by ANY angle, stamped onto a NEW layer (source "
        "left as-is) -- palette-legal by construction (introduces no new colours; right angles are "
        "exact). Params: angle (degrees, positive = clockwise, required); `rect` {x,y,width,height} OR "
        "`selection_only` (else the whole canvas); `at_x`/`at_y` (placement, default centred in place); "
        "`layer` (default \\\"Rotated\\\").",
    "live_dither_fill":
        "SPEC-009: ordered (Bayer) dither-fill a rectangle between two palette colours -- the tedious "
        "deterministic shading made palette-legal by construction (only color_a/color_b are emitted). "
        "Params: `rect` {x,y,width,height}; `color_a`/`color_b` (#rrggbb, usually two adjacent ramp "
        "steps); `level` (0..1 = fraction of color_b, default 0.5); `matrix` (\\\"bayer4\\\" default / "
        "\\\"bayer2\\\" / \\\"checker\\\"); `layer` (default AI Draft).",
    "live_create_autotile_template":
        "SPEC-003 Phase 3: compose an autotile sheet from FOUR corner quarters you drew as a "
        "left-to-right strip [fill | outer | edge | inner] (each tile_size/2 square; outer=convex "
        "top-left, edge=boundary on top, inner=concave top-left notch). layout=\\\"blob47\\\" (47 "
        "corner+edge tiles, default) or \\\"wang16\\\" (16 edge-only, inner unused). Drawn onto a new "
        "layer; then run live_pack_similar_tiles(grid_size=tile_size) to build the tileset. Params: "
        "tile_size (even 4..=64, required), layout, source_x/source_y (strip top-left, default 0,0), "
        "at_x/at_y, layer.",
}

src = open("src/server.rs", encoding="utf-8").read()
for tool, new in NEW.items():
    pat = re.compile(
        r'(#\[tool\(\s*description\s*=\s*)"(?:[^"\\]|\\.)*"(\s*\)\]\s*async fn ' + re.escape(tool) + r'\b)',
        re.S,
    )
    new_lit = '"' + new + '"'
    src, n = pat.subn(lambda m: m.group(1) + new_lit + m.group(2), src)
    assert n == 1, f"{tool}: matched {n} (expected 1)"
    print(f"{tool}: replaced -> {len(new)} chars")
open("src/server.rs", "w", encoding="utf-8").write(src)
print("server.rs updated")
