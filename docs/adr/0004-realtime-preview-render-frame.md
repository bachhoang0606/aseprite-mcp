# ADR 0004 — Live preview renders a single frame (no multi-frame save modal)

- Status: Accepted
- Date: 2026-06-14
- Checklist: 7.2 (auto-preview), 3.4 (live behaviour)
- Related: [ADR-0002](0002-decouple-ws-bridge.md), research `docs/research/agent-pixel-art-techniques.md` (Path 1)

## Context
The auto-preview hook calls `live_save_preview` after each live draw; its plugin
handler used `spr:saveCopyAs(filename)` to write the temp PNG that the Rust server
then upscales. On a **multi-frame** sprite this pops Aseprite's "file format does
not support multiple frames" **modal**, which blocks the UI thread → the command
`live_timeout`s and the stuck modal jams later commands. The only workaround was
the user ticking "Don't show this alert again" once — unacceptable for a realtime
experience.

Verified at Aseprite source (`src/app/file/file.cpp` `FileOp::createSaveDocumentOperation`):
the alert fires when `m_roi.frames() > 1` and the format lacks frame support, shown
via `OptionalAlert` gated on **`context->isUIAvailable()`** — which is TRUE inside
the GUI process the plugin runs in. The gate is **not** the `ui=false` param
(`cmd_save_file.cpp` shows `params().ui()` only gates the `ExportFileWindow`), and
`Sprite:saveCopyAs`'s Lua binding already sets `useUI=false` yet cannot suppress it.
**Conclusion: any path that saves the real multi-frame *sprite* to `.png` will pop
the modal.** The fix must change *what is saved*, not *how*.

## Decision (Tier A — shipped)
The preview handler renders the **active frame** into a standalone single-frame
`Image` and saves *that*:

```lua
local img = Image(spr.width, spr.height, ColorMode.RGB)
img:drawSprite(spr, app.frame.frameNumber)   -- lossless composite of visible layers
img:saveAs(filename)                          -- single image -> never multi-frame
```

`Image:saveAs` wraps the image in a temporary **one-frame** sprite, so
`roi.frames() > 1` is never true and the alert cannot fire — dialog-free by
construction, regardless of `isUIAvailable()`. This is exactly what
aseprite-mcp-pro does for its PNG path, and the in-memory render is what Pribambase
(the upstream aseprite#3009 author's live-sync plugin) does. A new
`handle_save_preview` is added next to `handle_save_copy_as`; the public
`live_save_copy_as` MCP tool is left untouched for explicit full-sprite exports.
`src/live.rs::save_preview` switches the one command name `save_copy_as → save_preview`;
the Rust upscale pipeline (`preview::render_preview`) is unchanged.

## Alternatives considered — deferred, with triggers
Tier A fully removes the modal. The following are **not** about the modal (A solves
that 100%); they trade complexity for a more "realtime" architecture.

**Tier B — pixel bytes over WS, no temp file.** The plugin renders to an in-memory
`Image` and ships `img.bytes` (raw buffer) over the existing WS; Rust builds the PNG
(the Pribambase model). Removes the disk round-trip and temp-file handoff.
- *Benefit:* marginal latency (sprites are KB; disk I/O on SSD is sub-ms) + cleaner
  internals (no temp artifact). User-imperceptible.
- *Cost:* byte/base64 transport over the JSON protocol, colorMode/`rowStride`-padding
  handling and indexed-palette resolution in Rust — re-implementing what Aseprite's
  PNG encoder gives for free.
- **Do it WHEN:** undertaking Tier C (the no-file path is its prerequisite), OR the
  temp-file handoff causes a real problem (cross-machine, permissions, leaked temp).
  **Not on its own** — the benefit isn't worth the code.

**Tier C — event-driven push (`sprite.events`).** The plugin listens to
`sprite.events:on('change', …)` (Timer-debounced) and pushes a preview whenever the
sprite changes, decoupled from the MCP tool-call. The preview becomes a **live
mirror** of the canvas.
- *Benefit:* (a) captures the **user's manual edits** in Aseprite, not just agent
  tool calls; (b) removes preview generation from the per-draw latency path; (c) a
  genuine "agent watches the canvas live" feel (Pribambase-style).
- *Key bound:* an LLM agent consumes previews in a **request-response cycle** — it
  only looks when it acts. So Tier C does NOT make the agent's *own* draw→review loop
  faster; its value is when a **human edits alongside the agent** (co-editing) or
  when the preview must always reflect non-tool-call changes.
- *Cost:* most complex; re-introduces a Timer debounce (the `change` event is a
  per-pixel firehose) and touches the **most regression-prone live state machine**
  (3 historical bugs: stuck `connecting`, ping teardown, orphan-socket races).
- **Do it WHEN:** the product targets **human+agent co-editing** or "agent watches
  the canvas", OR per-draw preview latency becomes a *measured* bottleneck for
  high-frequency drawing. For the current "agent draws → reviews its own work" loop,
  Tier A's on-demand fresh preview is the right granularity — **do not build C
  before that use case is real.**

## Consequences
- The multi-frame modal is eliminated for all sprites (multi-frame, indexed) with
  zero user interaction — the realtime complaint is resolved.
- The preview is a lossless RGB true-colour composite of the active frame (better
  for the vision model than an indexed snapshot); `preview::render_preview` converts
  to RGBA + upscales unchanged.
- `live_save_copy_as` remains the faithful full-sprite export (honours multi-frame
  semantics, may legitimately prompt for an explicit user export).
- Tier B/C are documented with concrete revisit triggers so they are picked up only
  when their distinct value (no-file transport / live-mirror co-editing) is needed.
