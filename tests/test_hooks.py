#!/usr/bin/env python3
"""End-to-end tests for the plugin hooks (checklist pillar 7).

Each hook is a standalone stdlib script driven by an event JSON on stdin. These
tests run the real scripts as subprocesses (UTF-8 stdin, like Claude Code does)
and assert their contract:

  7.1 guard_batch_draw.py  — blocks batch *drawing* (exit 2), allows live_*/export
                             /read (exit 0), honours the ASEPRITE_MCP_ALLOW_BATCH
                             opt-out, and never crashes on bad input.
  7.2 hooks.json           — the PostToolUse auto-preview mcp_tool hook is wired
                             to live_save_preview on the draw matcher.
  7.3 palette_lint_hook.py — warns (additionalContext) on a defective PNG, stays
                             quiet on a clean one, ignores non-PNG/missing paths.
  7.4 health_check.py      — emits valid SessionStart additionalContext JSON.

Run: python tests/test_hooks.py   (exit non-zero on failure; wired into CI)
"""
import json
import os
import subprocess
import sys
import unittest

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
HOOKS = os.path.join(ROOT, "hooks")
FIXTURES = os.path.join(ROOT, "tests", "visual", "fixtures")
PALETTE = os.path.join(ROOT, "knowledge", "palettes", "goblin-default.json")


def run_hook(script, event=None, env_extra=None, timeout=30):
    """Run a hook script, feeding `event` (dict) as UTF-8 JSON stdin."""
    env = dict(os.environ)
    env.setdefault("CLAUDE_PLUGIN_ROOT", ROOT)
    if env_extra:
        env.update(env_extra)
    stdin = "" if event is None else json.dumps(event)
    proc = subprocess.run(
        [sys.executable, os.path.join(HOOKS, script)],
        input=stdin, capture_output=True, text=True, encoding="utf-8",
        env=env, timeout=timeout,
    )
    return proc


class GuardBatchDraw(unittest.TestCase):
    def test_blocks_batch_drawing(self):
        for tool in ("mcp__aseprite__draw_rectangle", "mcp__aseprite__fill_area",
                     "mcp__aseprite__create_canvas", "mcp__aseprite__add_layer",
                     "mcp__aseprite-live__use_tool", "mcp__aseprite-live__new_cel"):
            p = run_hook("guard_batch_draw.py", {"tool_name": tool})
            self.assertEqual(p.returncode, 2, f"{tool} should be blocked")
            self.assertIn("BLOCKED", p.stderr)

    def test_blocks_batch_destructive(self):
        # 10.4: batch clear/remove/delete erase file content with no undo.
        for tool in ("mcp__aseprite__clear_cel", "mcp__aseprite__remove_frame",
                     "mcp__aseprite__remove_layer", "mcp__aseprite-live__delete_tag"):
            p = run_hook("guard_batch_draw.py", {"tool_name": tool})
            self.assertEqual(p.returncode, 2, f"{tool} should be blocked")
            self.assertIn("DESTRUCTIVE", p.stderr)

    def test_allows_live_export_and_read(self):
        for tool in ("mcp__aseprite-live__live_draw_pixels",
                     "mcp__aseprite-live__live_use_tool",
                     "mcp__aseprite-live__live_clear_cel",
                     "mcp__aseprite-live__live_delete_tag",
                     "mcp__aseprite__export_sprite",
                     "mcp__aseprite-live__get_sprite_info"):
            p = run_hook("guard_batch_draw.py", {"tool_name": tool})
            self.assertEqual(p.returncode, 0, f"{tool} should be allowed")

    def test_allow_batch_optout(self):
        p = run_hook("guard_batch_draw.py", {"tool_name": "mcp__aseprite__draw_rectangle"},
                     env_extra={"ASEPRITE_MCP_ALLOW_BATCH": "1"})
        self.assertEqual(p.returncode, 0, "opt-out env must let batch through")

    def test_bad_input_is_safe(self):
        # Malformed JSON must not crash or block.
        proc = subprocess.run(
            [sys.executable, os.path.join(HOOKS, "guard_batch_draw.py")],
            input="not json", capture_output=True, text=True, encoding="utf-8",
            env={**os.environ, "CLAUDE_PLUGIN_ROOT": ROOT}, timeout=30,
        )
        self.assertEqual(proc.returncode, 0)


class PaletteLintHook(unittest.TestCase):
    def test_warns_on_defective_png(self):
        png = os.path.join(FIXTURES, "bad_offpalette.png")
        p = run_hook("palette_lint_hook.py",
                     {"tool_input": {"filename": png}},
                     env_extra={"ASEPRITE_MCP_PALETTE": PALETTE})
        self.assertEqual(p.returncode, 0)
        self.assertTrue(p.stdout.strip(), "expected an additionalContext warning")
        data = json.loads(p.stdout)
        self.assertIn("palette-lint", data["hookSpecificOutput"]["additionalContext"])

    def test_quiet_on_clean_png(self):
        png = os.path.join(FIXTURES, "good_swatch.png")
        p = run_hook("palette_lint_hook.py",
                     {"tool_input": {"filename": png}},
                     env_extra={"ASEPRITE_MCP_PALETTE": PALETTE})
        self.assertEqual(p.returncode, 0)
        self.assertEqual(p.stdout.strip(), "", "clean sprite should produce no warning")

    def test_ignores_non_png_and_missing(self):
        for ti in ({"filename": "nope.txt"}, {"filename": os.path.join(FIXTURES, "missing.png")}, {}):
            p = run_hook("palette_lint_hook.py", {"tool_input": ti})
            self.assertEqual(p.returncode, 0)
            self.assertEqual(p.stdout.strip(), "")

    def test_skips_upscaled_preview(self):
        # live_save_preview output is upscaled; linting it would misfire on
        # stray/orphan-pixel checks, so the hook must skip it even on a defective PNG.
        png = os.path.join(FIXTURES, "bad_offpalette.png")
        p = run_hook("palette_lint_hook.py",
                     {"tool_name": "mcp__aseprite-live__live_save_preview",
                      "tool_input": {"filename": png}},
                     env_extra={"ASEPRITE_MCP_PALETTE": PALETTE})
        self.assertEqual(p.returncode, 0)
        self.assertEqual(p.stdout.strip(), "", "preview saves must not be linted")


class HealthCheck(unittest.TestCase):
    def test_emits_session_start_context(self):
        p = run_hook("health_check.py", event=None)
        data = json.loads(p.stdout)
        hso = data.get("hookSpecificOutput", {})
        self.assertEqual(hso.get("hookEventName"), "SessionStart")
        self.assertIn("additionalContext", hso)


class HooksManifest(unittest.TestCase):
    def setUp(self):
        with open(os.path.join(HOOKS, "hooks.json"), encoding="utf-8") as f:
            self.cfg = json.load(f)["hooks"]

    def test_lifecycle_events_present(self):
        for ev in ("SessionStart", "PreToolUse", "PostToolUse"):
            self.assertIn(ev, self.cfg)

    def test_guard_wired_on_aseprite_tools(self):
        pre = self.cfg["PreToolUse"][0]
        self.assertIn("aseprite", pre["matcher"])
        self.assertIn("guard_batch_draw.py", pre["hooks"][0]["command"])

    def test_auto_preview_mcp_tool_wired(self):
        # 7.2: a PostToolUse mcp_tool hook that re-saves a preview on each draw.
        blocks = self.cfg["PostToolUse"]
        mcp_hooks = [h for b in blocks for h in b["hooks"] if h.get("type") == "mcp_tool"]
        self.assertTrue(mcp_hooks, "expected an mcp_tool auto-preview hook")
        prev = mcp_hooks[0]
        self.assertEqual(prev["server"], "aseprite-live")
        # live_save_preview = save a faithful copy then nearest-neighbor upscale it
        # so a vision model can actually read the sprite (perception overhaul).
        self.assertEqual(prev["tool"], "live_save_preview")
        draw_block = next(b for b in blocks
                          if any(h.get("type") == "mcp_tool" for h in b["hooks"]))
        # matcher is a regex like live_(draw_pixels|use_tool|...); check it targets draws.
        self.assertIn("draw_pixels", draw_block["matcher"])
        self.assertIn("live_", draw_block["matcher"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
