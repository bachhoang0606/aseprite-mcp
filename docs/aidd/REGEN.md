# Regenerate-from-spec structure

> Checklist 12.5. The project is organized so each unit of behaviour maps 1:1 to a
> spec/ADR and can be **regenerated or re-reviewed from that source of truth**
> without spelunking the whole codebase. This is what makes the plugin durable:
> the spec is the contract; the code is one (replaceable) implementation of it.

## The 1:1 map (source of truth → regenerable unit)
| Source of truth | Regenerable unit | Regenerate by |
|---|---|---|
| `specs/SPEC-001` + `ADR-0002` | the WS bridge (`src/bin/aseprite-live-bridge.rs`) + control-client (`src/live.rs`) | re-implement the relay/handshake from the spec's Behaviour + Acceptance; verify with `tests/bridge_loopback.rs` |
| `ADR-0001` | live-vs-batch tool separation + `guard_batch_draw.py` | reconstruct the guard's block-list from the ADR; verify `evals/run.py::guard_decisions` |
| `ADR-0003` | `run_lua_script` posture + destructive-op handling | re-derive the gate/notes from the ADR |
| `rules/01..05` | `skills/pixel-*` behaviour + `agents/*` system prompts | rewrite a skill/agent from the rule it enforces; verify against `rules/06` rubric |
| `knowledge/palettes/*.json` | palette presets used by `pixel-palette` / `palette-smith` | regenerate ramps from the cited source; verify `palette_hueshift` eval |
| `tools/lint_sprite.py` docstring | the linter's detectors | re-implement detectors from the documented definitions; verify `linter_*` evals + `tests/visual/fixtures` |

## How to regenerate a unit
1. Open its spec/ADR (the **Behaviour** + **Acceptance criteria** are the contract).
2. Re-implement the module **only** from that contract (do not copy the old code).
3. Run the linked test/eval from [`TRACEABILITY.md`](TRACEABILITY.md); it must pass.
4. Re-score the linked checklist item; commit spec-ref + test-ref in the message.

## Coverage & honesty
- **Specced** (regen-ready): the WS bridge, batch-vs-live, security posture,
  rules→skills/agents, palettes, linter.
- **Not yet specced** (regen would need reverse-engineering — tracked debt):
  the bulk of `src/server.rs` live/batch tool surface and `src/tools/*` predate
  spec-first. New or changed tools must land a `specs/SPEC-NNN` first (12.1), so
  coverage grows toward full regen-from-spec rather than being claimed prematurely.

## Why this matters
A spec-first, 1:1-mapped layout means a contributor (or a model) can rebuild,
audit, or port any single piece from its spec and a green test — instead of the
whole thing being an opaque, all-or-nothing artifact. It is the difference between
a maintainable plugin and a one-shot script dump.
