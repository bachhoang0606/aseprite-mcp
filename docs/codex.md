# Using it with Codex (and other MCP clients)

This project is two things layered together:

1. **An MCP server** (`aseprite-live`) — an open-standard **tool provider** (`live_*`
   drawing/animation/perception tools). Works in **any** MCP client: Claude Code,
   **Codex**, Cursor, …
2. **A Claude Code plugin** (`aseprite-pixel-art`) — the `/pixel-*` slash commands,
   guard hooks, and subagents. This layer is **Claude-Code-specific**.

So on Codex you get the **tools natively**, but the `/pixel-*` commands do not exist —
Codex has its own equivalents (**skills** + `AGENTS.md`). This guide sets both up.

> Requires the live bridge to run: Aseprite open with the `aseprite-mcp-plugin`
> extension enabled, and on Windows 11 **Smart App Control OFF** (it silently blocks the
> unsigned bridge). See `/pixel-doctor` / `.agents/skills/pixel-doctor`.

## 1. Register the MCP server (tools)

Codex reads MCP servers from `~/.codex/config.toml` (TOML — **not** `.claude-plugin/`).
`${CLAUDE_PLUGIN_ROOT}` is a Claude-Code variable Codex won't expand, so use an
**absolute** path to a built `aseprite_mcp` that has `aseprite-live-bridge` next to it:

```toml
[mcp_servers.aseprite-live]
command = "C:\\path\\to\\aseprite-mcp\\target\\release\\aseprite_mcp.exe"

[mcp_servers.aseprite-live.env]
ASEPRITE_PATH = "C:\\Program Files\\Aseprite\\Aseprite.exe"
ASEPRITE_MCP_LIVE_PORT = "9876"
ASEPRITE_MCP_LIVE_CONTROL_PORT = "9877"
```

(macOS/Linux: drop `.exe`.) Or use the helper:
`codex mcp add aseprite-live --env ASEPRITE_PATH=<path> -- <abs path to aseprite_mcp>`.

Restart Codex; the `live_*` tools appear. You now drive them with plain prompts
("preflight, then draw a 24×24 goblin, palette-locked") — there are no `/pixel-*` slash
commands here.

## 2. Add the discipline (`AGENTS.md`)

Claude Code enforces preflight / palette-first / self-review via **hooks**; Codex has no
hooks, so the discipline must be written down. This repo's root **`AGENTS.md`** carries
it (tools, the non-negotiables, draw-by-code-not-generate, the machine gates, gotchas).
Codex reads `AGENTS.md` automatically when working in the repo. To apply it in another
project (e.g. your game repo), copy `AGENTS.md` to that repo's root.

## 3. Add the commands as Codex **skills**

Codex's replacement for the `/pixel-*` commands is **skills** (Markdown `SKILL.md` with
`name` + `description` frontmatter; Codex custom prompts are deprecated). This repo ships
them under **`.agents/skills/`**:

| Codex skill | ≈ Claude Code command |
|-------------|------------------------|
| `pixel-art` (umbrella / orchestrator) | the whole `/pixel-*` workflow |
| `pixel-new` | `/pixel-new` |
| `pixel-palette` | `/pixel-palette` |
| `pixel-animate` | `/pixel-animate` |
| `pixel-review` | `/pixel-review` |
| `pixel-generate` | `/pixel-generate` (image/sheet → disciplined pixels; uses Codex `$imagegen`) |
| `pixel-doctor` | `/pixel-doctor` |

Codex scans these locations (nearest wins): repo `.agents/skills/`, then
`$HOME/.agents/skills/` (user-wide), then `/etc/codex/skills`. To use the pixel skills in
**every** project, copy them to your user dir:

```bash
cp -r .agents/skills/pixel-* ~/.agents/skills/        # macOS/Linux
```
```powershell
Copy-Item .agents\skills\pixel-* $HOME\.agents\skills\ -Recurse   # Windows
```

Restart Codex, then invoke a skill explicitly with `$pixel-review`, `$pixel-animate`,
`$pixel-doctor` … or via the `/skills` menu — or let Codex pick one implicitly when a
task matches its description. Only the most-used verbs are split out; the `pixel-art`
umbrella covers the rest and multi-step "build a whole character" jobs.

## What differs from Claude Code

| | Claude Code (plugin) | Codex |
|---|---|---|
| MCP tools `live_*` | ✅ auto (plugin bundles the server) | ✅ manual (`~/.codex/config.toml`) |
| Commands | `/pixel-*` slash | `$pixel-*` skills (`.agents/skills/`) |
| Discipline enforcement | hooks (guard/lint/health) | `AGENTS.md` + skill text (self-enforced) |
| Python gates (`timing_lint`, `lint_sprite`, `silhouette_iou`) | shell | shell (run from this repo) |
| Image generation in `/pixel-generate` | no native tool on Claude Code | Codex `$imagegen` can serve the (opt-in) generation step |

## Troubleshooting
- **Skills don't show** → restart Codex; confirm they're under `.agents/skills/` (repo) or
  `~/.agents/skills/` (user); check the `/skills` menu.
- **`live_preflight` not ready / edits don't appear** → run the `pixel-doctor` skill:
  check port 9876 has a listener, Aseprite + extension are up, Smart App Control is OFF,
  and only one `aseprite_mcp.exe` is running.
- **Wrong server answers** → keep the name `aseprite-live` unique in `config.toml`.
