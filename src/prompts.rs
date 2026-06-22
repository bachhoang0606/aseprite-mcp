//! MCP prompt content (SPEC-010, roadmap #5).
//!
//! `pixel_creation_strategy` bakes this project's drawing doctrine into an MCP **prompt** the
//! client pulls, so every client (Cursor / Codex / Copilot, incl. non-vision) gets the
//! playbook — not just Claude-Code-skill users (research §B, the verified blender-mcp
//! `asset_creation_strategy()` pattern). Pure static content: no live bridge, no Aseprite, no
//! I/O, so it works even when the editor is disconnected. The hard-ordered beats below mirror
//! §A "Perception engineering" and §B "Workflow doctrine"; the unit tests guard each beat so
//! the doctrine can't silently rot.

/// The hard-ordered pixel-art creation playbook returned by the `pixel_creation_strategy`
/// MCP prompt.
pub const PIXEL_CREATION_STRATEGY: &str = r#"# Pixel-art creation strategy (Aseprite MCP)

Follow this order for ANY pixel-art drawing, editing, or animation task. It exists because
the usual failure mode is the model confidently editing a canvas it cannot actually see, with
hand-picked colours, and never re-checking. Work the steps in order; do not skip perception.

## 0. Connect first (live-first)
Call `live_preflight` (or `live_status`) and only use the `live_*` tools once `connected` is
true. If it is NOT connected, STOP and tell the user the live Aseprite session is not
connected — do not silently write to disk instead, because file edits will not show up in the
open editor. To start a fresh canvas, seed a blank PNG and `live_open_sprite` it (the New File
command is modal and hangs).

## 1. Perceive before you act — you cannot see the canvas you imagine
- A raw 32×32 preview is ~4 vision patches of signal — effectively invisible. ALWAYS render
  with `live_save_preview` (it upscales the long edge to ~1024px, the grounding sweet spot, and
  returns an inline image) and LOOK at it before deciding anything.
- Pair every image with machine-truth: `live_ascii_view` (one token per pixel — models read
  grids far better than they write them), plus `live_get_sprite_info` / `live_list_layers` /
  `live_list_frames` / `live_list_palette`. Never trust your memory of the canvas state.
- Use the labelled coordinate gutter to name the exact (x,y) you intend to change; rely on
  chunky 8px guides, never 1px hairlines.

## 2. Lock a palette, then never invent colours
- Choose/lock a palette FIRST (`/pixel-palette`, `live_set_palette_color`, `live_list_palette`).
- Snap to it with the real CIELAB tools (`live_palette_snap`, `live_snap_colors`,
  `live_adjust_pixels`) instead of hand-picking shades. Hand-computing slightly-wrong colours is
  the #1 failure; let the tools pick the nearest legal colour.

## 3. Prefer constrained / semantic tools over hand-plotting pixels
LLMs emit correct *operations* far better than correct pixel coordinates. Reach for:
- `live_import_reference` — turn a photo / AI image / CC0 asset into clean, palette-locked
  pixels you trace over (use `regrid:true` for a scaled/"fake" reference). Start from a
  reference whenever the shape is hard to invent.
- `live_dither_fill` (palette-legal gradients), `live_gradient_map` (re-shade onto a ramp),
  `live_rotate` (artifact-free), and the tilemap / `live_stamp_tiles` family.
Draw freehand pixels (`live_draw_pixels`, `live_use_tool`) only for small, deliberate touch-ups
on the AI-draft layer.

## 4. Ramp & light discipline
- Build value ramps: monotone value (dark → light), hue-shift across the ramp (warmer
  highlights, cooler shadows), and ONE consistent light direction. 3–5 steps per ramp.
- To "match my existing sheet", derive the contract with `live_extract_style_profile`
  (grid / palette / ramps / light_dir / outline policy) and stay inside it.

## 5. Re-perceive after EVERY change — before and after
- Re-render with `live_save_preview` and look again. As context fills, models congratulate
  themselves on art that is objectively broken; an external look is the only antidote.
- Diff edits with `live_frame_diff`. Review animation with `live_save_filmstrip` (a single
  composite image) — the Claude API sees only the FIRST frame of an animated GIF, so never
  "review the GIF". Watch for cross-frame proportion drift.

## 6. Validate against a hard gate
Run the linter / review (the `/pixel-review` skill, ramp-lint, silhouette checks) and treat the
result as a HARD gate, not as advice. Fix what it flags before declaring done.

## 7. Script is the last resort
Drawing edits are applied through the plugin: the content and create/delete ops
(`live_draw_pixels`, cels, frames, tags, snap/adjust) run inside a named `app.transaction`, so
the human watching gets a clean Ctrl+Z per action. The `run_lua_script` / `execute_cli` escape
hatch runs arbitrary code, is disabled unless `ASEPRITE_MCP_ALLOW_LUA=1`, and runs offline/batch
(not in the live session). Prefer a first-class tool; if you must script, save first and keep the
change small and reviewable.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// The doctrine must keep each hard-ordered beat — a guard against silent rot. Each tuple
    /// is (human label, a substring that must appear).
    #[test]
    fn doctrine_covers_every_ordered_beat() {
        let beats = [
            ("live-first / preflight", "live_preflight"),
            ("perceive via preview", "live_save_preview"),
            ("machine-truth ascii", "live_ascii_view"),
            ("lock + snap palette", "live_palette_snap"),
            ("constrained: import_reference", "live_import_reference"),
            ("constrained: dither", "live_dither_fill"),
            ("constrained: gradient_map", "live_gradient_map"),
            ("ramp/style profile", "live_extract_style_profile"),
            ("re-perceive: frame_diff", "live_frame_diff"),
            ("animation: filmstrip", "live_save_filmstrip"),
            ("first GIF frame caveat", "FIRST frame"),
            ("hard gate review", "/pixel-review"),
            ("script last / gate env", "ASEPRITE_MCP_ALLOW_LUA"),
            ("transaction undo", "app.transaction"),
        ];
        for (label, needle) in beats {
            assert!(
                PIXEL_CREATION_STRATEGY.contains(needle),
                "doctrine is missing the '{label}' beat (substring {needle:?})"
            );
        }
    }

    /// It must read as an ordered playbook (the numbered steps), not a loose blob.
    #[test]
    fn doctrine_is_hard_ordered() {
        for step in ["## 0.", "## 1.", "## 2.", "## 3.", "## 4.", "## 5.", "## 6.", "## 7."] {
            assert!(PIXEL_CREATION_STRATEGY.contains(step), "missing ordered step {step}");
        }
    }
}
