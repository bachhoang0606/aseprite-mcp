# SPEC-002 — Opt-in gate for arbitrary Lua/CLI execution

- Status: Implemented
- Owner: project
- Checklist items advanced: 10.1 (run_lua_script risk controlled), 10.2 (localhost
  binding asserted), 10.3 (no telemetry/secrets documented), 10.4 (destructive ops)
- Related ADRs: [ADR-0003](../docs/adr/0003-run-lua-script-security.md),
  [ADR-0001](../docs/adr/0001-batch-vs-live-tools.md)

## Intent
`run_lua_script` and `execute_cli` run an unrestricted Aseprite `--batch --script`
process — effectively arbitrary code execution on the user's machine, the largest
security surface in the project. Previously they were always callable, relying on
agent discipline alone. This feature makes that dangerous surface **off by default**
so an agent cannot run unreviewed Lua/CLI unless the operator explicitly opts in,
without affecting the safe `live_*` drawing path.

## Inputs / Outputs
- **Input:** env var `ASEPRITE_MCP_ALLOW_LUA` in the MCP server process
  (truthy = `1`/`true`/`yes`/`on`, case-insensitive; anything else / unset = off).
- **Output:** when the gate is closed, `run_lua_script` / `execute_cli` return an
  actionable error (names the tool, the env var, and the security docs) and do
  **not** spawn Aseprite. When open, behaviour is unchanged.

## Behaviour
1. `lua_execution_allowed()` reads `ASEPRITE_MCP_ALLOW_LUA` and returns true only
   for a truthy value (`is_truthy`).
2. `run_lua_script` and `execute_cli` check the gate first; if closed they return
   `lua_disabled_error(tool)` before any process spawn.
3. The gate is **only** on these two arbitrary-execution tools. Typed batch tools
   (`export_sprite`, `export_spritesheet`, …) route through `run_cli` directly and
   are unaffected — they take fixed, safe arguments.
4. Network exposure is independently constrained: the bridge binds `127.0.0.1`
   only (`loopback_addr`), never an all-interfaces address.
5. No telemetry/secrets: the server has no remote calls and reads only local
   path/port env vars (documented in SECURITY.md).

## Acceptance criteria
- [x] With the var unset, `run_lua_script`/`execute_cli` refuse with an actionable
      error and spawn nothing.
- [x] With `ASEPRITE_MCP_ALLOW_LUA=1`, both run as before.
- [x] Truthy/falsy parsing is correct and case-insensitive.
- [x] `live_*` drawing and typed export tools are unaffected by the gate.
- [x] The bridge bind address is loopback-only (regression-tested).
- [x] SECURITY.md documents the gate, localhost binding, telemetry/secrets, and
      destructive-op reversibility.

## Eval (how we grade it)
Deterministic Rust unit tests (no Aseprite needed):
`src/tools/scripting.rs::tests` — `truthy_values_open_the_gate`,
`falsy_or_unset_keeps_gate_closed`, `disabled_error_is_actionable`;
`src/bin/aseprite-live-bridge.rs::tests::bind_address_is_loopback_only`.

## Traceability
- Module(s): `src/tools/scripting.rs` (`lua_execution_allowed`, `is_truthy`,
  `lua_disabled_error`, gate in `run_lua_script`/`execute_cli`); `src/server.rs`
  (tool descriptions); `src/bin/aseprite-live-bridge.rs` (`loopback_addr`).
- Test(s): `src/tools/scripting.rs::tests`,
  `src/bin/aseprite-live-bridge.rs::tests::bind_address_is_loopback_only`.
- Docs: `SECURITY.md`, `docs/adr/0003-run-lua-script-security.md`.
