# SPEC-010 — `pixel_creation_strategy` MCP prompt (+ transaction/snapshot confirmation)

- Status: **Implemented (2026-06-22)**
- Owner: project
- Checklist items advanced: 2.x (new MCP surface — first **prompt**, not a tool), 10.x
  (workflow doctrine / escape-hatch discipline)
- Related ADRs: ADR-0003 (run_lua_script security gate); ADR-0005 (loud degradation). No new
  ADR — adds an MCP **prompt** (a pull-only, read-only capability), no new return-type or
  protocol contract beyond enabling the standard `prompts` capability.
- Source: research doc [`docs/research/agent-pixel-art-techniques.md`](../docs/research/agent-pixel-art-techniques.md)
  §B "Workflow doctrine" (the verified blender-mcp `asset_creation_strategy()` pattern; line ~303),
  §A "Perception engineering", roadmap **#5**.

## Intent
The project's drawing doctrine — live-first, perception-first, palette/ramp discipline, prefer
constrained tools over hand-plotting, validate against a hard gate, script as last resort — is
**scattered across skill files** that only Claude-Code-skill users see (§B). blender-mcp proved
that baking the workflow into an **MCP prompt** the client pulls (`asset_creation_strategy()`)
materially improves agent behaviour, and that every client (Cursor / Codex / Copilot, including
non-vision ones) gets it. This spec ships the analogous **`pixel_creation_strategy`** prompt
**from the Rust server** so the playbook travels with the server, not the skill bundle.

Secondary: **confirm** the §B "transaction-scoped undo" and "auto-snapshot before script"
recommendations against the current implementation (the roadmap framed this as "may already
exist") and document the result.

## Inputs / Outputs
- **Inputs:** none. `pixel_creation_strategy` is a no-argument MCP prompt; the client lists it
  (`prompts/list`) and pulls it (`prompts/get`).
- **Outputs:** a single `PromptMessage` (role `user`) carrying the doctrine as Markdown — a
  hard-ordered playbook the client injects into the conversation. Pure static content (no live
  bridge, no Aseprite, no I/O), so it works even when the editor is disconnected.

## Behaviour
- The server advertises the `prompts` capability (`ServerCapabilities::…enable_prompts()`) and
  registers exactly one prompt, `pixel_creation_strategy`, via an rmcp `#[prompt_router]`
  block; `ServerHandler::list_prompts` / `get_prompt` delegate to that `PromptRouter` (mirroring
  the existing manual `list_tools` / `call_tool` wiring, because the server keeps a custom
  `get_info` with instructions — so the auto-`get_info` `#[prompt_handler]` macro is not used).
- The prompt body (`prompts::PIXEL_CREATION_STRATEGY`) hard-orders the workflow:
  0. **Connect first** — `live_preflight` / `live_status`; only use `live_*` once `connected`;
     never silently write to disk if disconnected.
  1. **Perceive before acting** — a raw 32² preview ≈ 4 vision patches ≈ invisible; always
     `live_save_preview` (auto-upscales the long edge to ~1024px, the grounding sweet spot) and
     look at the inline image; pair it with machine-truth (`live_ascii_view` — one token per
     pixel; `live_get_sprite_info`, `live_list_*`); use the labelled gutter; never trust memory.
  2. **Lock a palette, never invent colours** — set the palette first; snap with the real CIELAB
     tools (`live_palette_snap` / `live_snap_colors` / `live_adjust_pixels`), not hand-picked shades.
  3. **Prefer constrained / semantic tools** over hand-plotting pixels — `live_dither_fill`,
     `live_gradient_map`, `live_rotate`, `live_import_reference` (with `regrid:true` for scaled
     refs), tilemap/stamp tools; start from a reference when shape is hard to invent.
  4. **Ramp & light discipline** — monotone-value ramps, hue-shift, one light direction, 3–5
     steps; derive/lint with `live_extract_style_profile`.
  5. **Re-perceive after every change (before AND after)** — re-render and look; `live_frame_diff`;
     review animation with `live_save_filmstrip` (a single composite — the Claude API sees only
     the FIRST GIF frame, so never "review the GIF"); watch cross-frame proportion drift.
  6. **Validate against a hard gate** — run the linter / `/pixel-review`; treat it as a gate, not advice.
  7. **Script is the last resort** — prefer a first-class tool; `run_lua_script` / `execute_cli`
     run arbitrary code, are off by default (`ASEPRITE_MCP_ALLOW_LUA=1`), and are offline/batch.

### Transaction / snapshot — confirmed state (audited, no code change in this spec)
Audited `scripts/aseprite-mcp-plugin/plugin.lua` (18 `app.transaction` blocks across 60
handlers). The picture is **partial, not universal** (the original "every mutating op is wrapped"
draft was an overclaim, caught in review):
- **Wrapped (named `app.transaction("MCP Live <Op>", …)`):** the content edits and create/delete
  ops — `handle_draw_pixels` ("MCP Live Draw Pixels"), `use_tool`, `clear_cel` / `new_cel` /
  `delete_cel`, `ensure_frames` / `new_empty_frame` / `new_frame` / `delete_frame`,
  `new_tag` / `delete_tag`, `new_slice` / `delete_slice`, `adjust_pixels`, `snap_colors`, and the
  tilemap/tile ops. These already deliver the §B "one Ctrl+Z per agent action" win.
- **NOT wrapped (a documented gap → follow-up):** the in-place **property setters** and a few
  structural ops mutate via direct API assignment without a transaction —
  `set_layer_properties`, `set_layer_visibility`, `rename_layer`, `create_group_layer`,
  `delete_layer`, `set_cel_properties`, `set_frame_properties`, `set_tag_properties`,
  `set_slice_properties`, `set_palette_color`, `resize_palette`, `resize_canvas`,
  `set_sprite_properties`. Wrapping these uniformly is a worthwhile **plugin follow-up** (some
  interleave validation early-returns with mutation and need a validate-then-transact restructure,
  and a plugin change needs live verification), so it is **out of scope for this prompt-focused
  spec** and tracked here rather than silently claimed as done.
- **Auto-snapshot before `run_lua_script`: not applicable as a live snapshot.** Unlike
  blender-mcp's `execute_blender_code` (which mutates the *live* scene), our `run_lua_script` /
  `execute_cli` run **offline batch** processes (`aseprite --batch --script` on a file, a separate
  process — `src/tools/scripting.rs`), so there is no live undo stack to snapshot; the protection
  is the ADR-0003 `ASEPRITE_MCP_ALLOW_LUA` gate plus out-of-process file isolation. The doctrine
  prompt encodes the residual guidance ("save first, keep scripts small/reviewable, prefer a
  first-class tool"). → **Confirmed; no code change.**

## Acceptance criteria
- [x] The server advertises the `prompts` capability and `prompts/list` returns exactly one
      prompt named `pixel_creation_strategy`; `prompts/get` returns its doctrine message.
- [x] The prompt is pure static content (no live bridge / Aseprite / I/O) — works disconnected.
- [x] Doctrine content covers the hard-ordered beats (preflight, perceive/preview-at-scale,
      lock-palette, constrained tools, ramp/light, re-perceive before+after, hard-gate validate,
      script-last) — guarded by a content unit test so the doctrine cannot silently rot.
- [x] `prompt_router().list_all()` lists the prompt (registration test); content invariants tested.
- [x] No new dependency. `cargo test --bins` passes. Transaction/snapshot state confirmed above.

## Eval (how we grade it)
- **Deterministic (Tier-A, no Aseprite):** unit tests — the prompt is registered with the exact
  name; `PIXEL_CREATION_STRATEGY` contains each ordered doctrine beat (preflight, save_preview,
  palette snap, import_reference/dither/gradient, ramp/light, filmstrip/frame_diff, hard gate,
  run_lua_script-last).
- **Live (Tier-B, manual):** in a fresh client, `prompts/list` shows `pixel_creation_strategy`;
  pulling it injects the playbook; a subsequent "draw me a 32×32 hero" run follows the order
  (preflight → preview → palette → … → review) rather than blind pixel emission.

## Traceability
- Module(s): `src/prompts.rs` (the `PIXEL_CREATION_STRATEGY` doctrine const), `src/server.rs`
  (`#[prompt_router]` block with `pixel_creation_strategy`; `PromptRouter` field;
  `enable_prompts()` in `get_info`; manual `list_prompts` / `get_prompt` delegation). No
  `plugin.lua` change (content/create-delete ops already transaction-wrapped; the property-setter
  wrapping gap is tracked above as a follow-up, not done here); no new tool.
- Test(s): `src/prompts.rs::tests` (content beats), `src/server.rs::tests`
  (`prompt_router` registers `pixel_creation_strategy`).
