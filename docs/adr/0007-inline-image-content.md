# ADR 0007 — `live_save_preview` may return the PNG as inline MCP image content

- Status: Accepted
- Date: 2026-06-20
- Checklist: 1.x (perception/preview surface), 7.2 (auto-preview)
- Context: [SPEC-005](../../specs/SPEC-005-perception-fast-follow.md) Phase 3, research
  `docs/research/agent-pixel-art-techniques.md` (Path 1 / §A, roadmap #1 "return image
  inline (MCP Image content)")
- Related: [ADR-0004](0004-realtime-preview-render-frame.md) (the preview render),
  [ADR-0005](0005-tilemap-protocol-and-bitmask.md) (loud per-command degradation)

## Context
`live_save_preview` upscales the active frame and (Phases 1–2) composites a coordinate
gutter / crops to the subject, then writes a PNG and returns a **JSON string** with the
path + metadata. A vision client must be *told* to open that path; Claude Code's
auto-preview hook does, but other MCP clients (Cursor / Codex on the same server) often
won't — so the pixels never reach the model even though we just rendered them. Roadmap
item #1 calls for returning the PNG **inline** as an MCP image-content block so the
pixels land in context directly.

MCP lets a tool result carry a vector of content blocks (text, image, …). In `rmcp`
0.3.2 a `#[tool]` fn returning `Result<CallToolResult, McpError>` can emit
`CallToolResult::success(vec![Content::text(..), Content::image(base64, "image/png")])`.
Every existing tool returns `Result<String, String>` (→ one text block), so this is the
**first** tool in the crate to emit image content — hence this ADR.

## Decision
1. **Inline is opt-in; the path is always present.** Default behaviour is unchanged: a
   single text block with the JSON sidecar (the auto-preview hook and non-vision clients
   depend on the path). `inline:true` *adds* an `image/png` block alongside it. The tool
   return type changes from `Result<String, String>` to `Result<CallToolResult, McpError>`;
   the no-inline wire shape (one text block) is byte-identical to before, so existing
   clients and the hook are unaffected.
2. **Byte-ceiling degrade, never silent truncation.** A preview over `INLINE_MAX_BYTES`
   (1 MiB of PNG ≈ ~1.4 MiB base64) does **not** inline — it appends a short text *note*
   (size + the path) instead, so a large sheet can't blow the model's context budget. A
   read error degrades the same way. The path is always usable. This mirrors ADR-0005's
   "degrade loudly, never silently."
3. **The assembly is a pure, testable seam.** The live method `LiveBridge::save_preview`
   still returns the JSON string (path + metadata) unchanged; the image concern lives at
   the server boundary (`build_preview_call_result`), and the file-read + size-guard +
   encode is the pure `preview::read_inline_png` → `InlinePng::{Ready, TooLarge}`,
   unit-tested without the bridge.
4. **Base64 is hand-rolled (`preview::base64_encode`), not a new dependency.** Encode is
   the only thing the inline path needs, the encoder is ~15 lines pinned to the RFC 4648
   vectors, and it keeps the crate's dependency tree lean (matching the `image`
   png-only posture). It also sidesteps a concrete local-dev hazard: adding a crate to
   `Cargo.toml` forces a fuller relink that Windows Smart App Control blocks on the
   freshly-built **test** binary (os error 4551), which a code-only rebuild does not —
   so `cargo test --bins` stays runnable locally.

## Alternatives considered
- **Always inline (no opt-in).** Rejected: doubles every preview's payload, breaks the
  context-budget guarantee for big sheets, and the auto-preview hook only needs the file
  on disk — paying image tokens on every post-draw preview is waste.
- **Inline `live_get_tileset` / `live_save_filmstrip` now too.** Deferred: they already
  produce vision PNGs and can reuse `read_inline_png` + `build_preview_call_result`
  verbatim, but the acceptance scope is `live_save_preview`; fold them in when a client
  needs it rather than widening the return-type change pre-emptively.
- **Pull the `base64` crate.** Rejected for (4) above — already in the lock tree
  transitively via `rmcp`, but declaring a direct dep buys nothing over ~15 tested lines
  and reintroduces the SAC relink block.

## Consequences
- A vision client that sets `inline:true` sees the upscaled, gutter-labelled,
  cropped preview pixels **in context** — no "open this file" round-trip — closing the
  see→locate→fix loop for non-Claude-Code clients.
- The default path is unchanged (text + path); the auto-preview hook and existing
  clients keep working with no behavioural change.
- `live_save_preview` is the reference for any future image-returning tool: opt-in,
  path-always-present, byte-ceiling degrade, pure assembly seam.
- No new dependency; `cargo test --bins` remains runnable under Windows SAC.
- Capability advertisement (`perception2`) is still deferred — `inline` is a pure
  server-side option needing no plugin support, so nothing here is version-gated.
