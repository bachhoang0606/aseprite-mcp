#!/usr/bin/env python3
"""PreToolUse hook: block silent batch *drawing* and *destructive* fallback
(checklist 7.1, 10.4).

Enforces ADR-0001 (docs/adr/0001-batch-vs-live-tools.md): batch/headless tools
edit files on disk and do NOT appear in the open Aseprite window, so they must
never be used to "work around" a disconnected live session. This guard blocks
canvas-mutating BATCH tools (anything that draws/creates pixels that is not a
`live_*` tool) and DESTRUCTIVE batch tools (clear_/remove_/delete_), which
erase file content with no in-app undo (ADR-0003: live_* equivalents stay
allowed because Aseprite's undo history makes them reversible). Read-only
batch (get_/list_) and explicit export remain allowed.

Opt out for a deliberate offline-generation task with:
    set ASEPRITE_MCP_ALLOW_BATCH=1   (Windows)
    export ASEPRITE_MCP_ALLOW_BATCH=1 (mac/linux)

Stdlib-only. Exit 2 + stderr blocks the tool and feeds the reason back to Claude.
"""
import json
import os
import sys

# Batch action names that paint/create pixels (the dangerous silent-disk set).
# use_tool/new_cel are the batch counterparts of live_use_tool/live_new_cel;
# create_tag/create_slice stay allowed (metadata, not pixels).
BLOCK_EXACT = {
    "create_canvas",
    "create_sprite",
    "create_cel",
    "new_cel",
    "use_tool",
    "add_layer",
    "add_frame",
    "add_frames",
    "apply_gradient_rect",
}
BLOCK_PREFIXES = ("draw", "fill", "paint")
# Destructive batch ops: erase content from the file on disk with no undo
# (checklist 10.4). live_clear_*/live_delete_* stay allowed — undoable in-app.
DESTRUCTIVE_PREFIXES = ("clear_", "remove_", "delete_")


def is_blocked(tool_name: str) -> bool:
    # Only the two Aseprite MCP servers are relevant.
    if "aseprite" not in tool_name:
        return False
    short = tool_name.split("__")[-1]
    # Live tools are exactly what we WANT; never block them.
    if short.startswith("live_"):
        return False
    if short in BLOCK_EXACT:
        return True
    return any(short.startswith(p) for p in BLOCK_PREFIXES + DESTRUCTIVE_PREFIXES)


def main() -> None:
    allow = os.environ.get("ASEPRITE_MCP_ALLOW_BATCH", "").strip().lower()
    if allow in ("1", "true", "yes", "on"):
        sys.exit(0)

    try:
        data = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        sys.exit(0)  # Can't parse — don't get in the way.

    tool_name = data.get("tool_name", "")
    if not is_blocked(tool_name):
        sys.exit(0)

    short = tool_name.split("__")[-1]
    kind = ("DESTRUCTIVE (deletes content with no undo)"
            if short.startswith(DESTRUCTIVE_PREFIXES) else "drawing")
    sys.stderr.write(
        f"BLOCKED: '{tool_name}' is a BATCH (headless) {kind} tool - it edits a "
        f"file on disk and will NOT appear in the open Aseprite window. Do not use "
        f"it to work around a disconnected live session. Instead: call live_preflight, "
        f"then use the live_* equivalent (undoable in Aseprite). If this truly is an "
        f"explicit offline file-generation task the user asked for, set "
        f"ASEPRITE_MCP_ALLOW_BATCH=1 and retry. (ADR-0001, ADR-0003)\n"
    )
    sys.exit(2)


if __name__ == "__main__":
    main()
