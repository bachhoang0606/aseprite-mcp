#!/usr/bin/env python3
"""PostToolUse hook: lint a saved/exported sprite PNG (checklist 7.3).

When a save/export tool writes a PNG, run the deterministic sprite linter
(tools/lint_sprite.py) on it and surface any structural defects (orphan/stray
pixels, off-palette colours if a project palette is configured) back to Claude as
context. Non-blocking: always exits 0; it only *warns*, never fails the tool.

Stdlib-only. Reads the PostToolUse event JSON on stdin.
"""
import json
import os
import subprocess
import sys


def find_saved_path(data: dict):
    ti = data.get("tool_input", {}) or {}
    for key in ("filename", "file_path", "path", "output", "outputPath"):
        v = ti.get(key)
        if isinstance(v, str) and v:
            return v
    # some tools echo the written path in tool_response
    tr = data.get("tool_response", {}) or {}
    if isinstance(tr, dict):
        for key in ("filename", "path"):
            v = tr.get(key)
            if isinstance(v, str) and v:
                return v
    return None


def main() -> None:
    try:
        data = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        sys.exit(0)

    # live_save_preview writes a nearest-neighbor *upscaled* PNG for the vision
    # model; linting it is wrong (every source pixel is now an NxN block, so
    # stray/orphan-pixel detection misfires). Real deliverable saves/exports are
    # 1x and still get linted.
    if str(data.get("tool_name", "")).endswith("live_save_preview"):
        sys.exit(0)

    path = find_saved_path(data)
    if not path or not path.lower().endswith(".png") or not os.path.exists(path):
        sys.exit(0)  # only lint PNGs that actually exist

    root = os.environ.get("CLAUDE_PLUGIN_ROOT", os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    linter = os.path.join(root, "tools", "lint_sprite.py")
    if not os.path.exists(linter):
        sys.exit(0)

    # Optional project palette for off-palette detection.
    # Scope this passive save-time advisory to structural defects (orphan/off-palette);
    # the size-derived colour budget is a /pixel-review concern, not a per-save alarm.
    palette = os.environ.get("ASEPRITE_MCP_PALETTE")
    cmd = [sys.executable, linter, path, "--warn-only", "--no-auto-budget"]
    if palette and os.path.exists(palette):
        cmd += ["--palette", palette]

    try:
        out = subprocess.run(cmd, capture_output=True, text=True, timeout=15)
        report = json.loads(out.stdout or "{}")
    except Exception:
        sys.exit(0)

    counts = report.get("counts", {})
    if not counts:
        sys.exit(0)  # clean — stay quiet

    parts = ", ".join(f"{n}x {t}" for t, n in counts.items())
    msg = (
        f"palette-lint on {os.path.basename(path)}: {parts}. "
        f"These are deterministic structural findings (stray/orphan pixels"
        + (", off-palette colours" if palette else "")
        + "). Review with /pixel-review or pixel-critic; banding/pillow-shading are not auto-detected."
    )
    print(
        json.dumps(
            {"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": msg}}
        )
    )


if __name__ == "__main__":
    main()
