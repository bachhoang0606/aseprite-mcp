#!/usr/bin/env python3
"""Tests for scripts/pixel_doctor.py — deterministic, stdlib-only, OFFLINE.

Run: python tests/test_pixel_doctor.py   (exit non-zero on failure; CI-wired)
Covers only the pure diagnosis core (filesystem/mtime/which are injected); the live env
probes (winreg SAC, socket ports, real ~/.claude.json) are best-effort and not unit-tested.
"""
import os
import sys
import unittest

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

import pixel_doctor as d  # noqa: E402

SERVER, BRIDGE = d.SERVER_BIN, d.BRIDGE_BIN


def status_of(findings, check):
    return [f["status"] for f in findings if f["check"] == check]


class FindServer(unittest.TestCase):
    def test_finds_nested_live_server_and_shadow(self):
        cfg = {"mcpServers": {"x": {}},
               "projects": {"/p": {"mcpServers": {"aseprite-live": {"command": "C"}, "aseprite": {}}}}}
        live, shadow = d.find_live_server(cfg)
        self.assertEqual(live, {"command": "C"})
        self.assertTrue(shadow)

    def test_no_live_server(self):
        live, shadow = d.find_live_server({"mcpServers": {"other": {}}})
        self.assertIsNone(live)
        self.assertFalse(shadow)


class DiagnoseCommand(unittest.TestCase):
    def setUp(self):
        self.hyphen = os.path.join("work", "aseprite-mcp")
        self.base = os.path.join("R", "target", "release")
        self.server = os.path.join(self.base, SERVER)
        self.bridge = os.path.join(self.base, BRIDGE)

    def test_healthy(self):
        out = d.diagnose_command(self.server, self.hyphen, lambda p, fs={self.server, self.bridge}: p in fs)
        self.assertEqual(status_of(out, "claude.json/exe"), [d.OK])
        self.assertEqual(status_of(out, "sibling-bridge"), [d.OK])
        self.assertEqual(status_of(out, "wrong-repo"), [])  # hyphen path, no warning

    def test_missing_command(self):
        out = d.diagnose_command("", self.hyphen, lambda p: True)
        self.assertEqual(out[0]["status"], d.FAIL)

    def test_missing_exe(self):
        out = d.diagnose_command(self.server, self.hyphen, lambda p: False)
        self.assertEqual(status_of(out, "claude.json/exe"), [d.FAIL])

    def test_missing_sibling_bridge(self):
        out = d.diagnose_command(self.server, self.hyphen, lambda p, only={self.server}: p in only)
        self.assertEqual(status_of(out, "sibling-bridge"), [d.FAIL])

    def test_wrong_repo_warns(self):
        ubase = os.path.join("u", "aseprite_mcp", "target", "release")
        us, ub = os.path.join(ubase, SERVER), os.path.join(ubase, BRIDGE)
        out = d.diagnose_command(us, self.hyphen, lambda p, fs={us, ub}: p in fs)
        self.assertEqual(status_of(out, "wrong-repo"), [d.WARN])

    def test_custom_build_dir_is_not_flagged(self):
        # The real machine uses target/rmcp1 — co-located bridge is fine, no false positive.
        cbase = os.path.join("R", "target", "rmcp1")
        cs, cb = os.path.join(cbase, SERVER), os.path.join(cbase, BRIDGE)
        out = d.diagnose_command(cs, self.hyphen, lambda p, fs={cs, cb}: p in fs)
        self.assertEqual(status_of(out, "sibling-bridge"), [d.OK])
        self.assertEqual(status_of(out, "wrong-repo"), [])


class LocateAseprite(unittest.TestCase):
    def test_resolution_order(self):
        self.assertEqual(
            d.locate_aseprite({"ASEPRITE_PATH": "/a"}, [], lambda p: p == "/a", lambda n: None),
            ("/a", "ASEPRITE_PATH"))
        # ASEPRITE_PATH set but not on disk -> fall through to candidates.
        self.assertEqual(
            d.locate_aseprite({"ASEPRITE_PATH": "/gone"}, ["/c"], lambda p: p == "/c", lambda n: None),
            ("/c", "install-dir"))
        # else PATH.
        self.assertEqual(
            d.locate_aseprite({}, [], lambda p: False, lambda n: "/onpath"), ("/onpath", "PATH"))
        # else nothing.
        self.assertEqual(d.locate_aseprite({}, [], lambda p: False, lambda n: None), (None, None))


class StaleCheck(unittest.TestCase):
    def test_older_registered_warns(self):
        f = d.stale_check("/old", "/new", lambda p: True, lambda p: {"/old": 1.0, "/new": 2.0}[p])
        self.assertEqual(f["status"], d.WARN)

    def test_same_path_ok(self):
        f = d.stale_check("/x", "/x", lambda p: True, lambda p: 1.0)
        self.assertEqual(f["status"], d.OK)

    def test_newer_is_info(self):
        f = d.stale_check("/new", "/old", lambda p: True, lambda p: {"/old": 1.0, "/new": 2.0}[p])
        self.assertEqual(f["status"], d.INFO)

    def test_missing_returns_none(self):
        self.assertIsNone(d.stale_check("/a", "/b", lambda p: False, lambda p: 1.0))


class ClassifyPreflight(unittest.TestCase):
    def test_connected(self):
        self.assertEqual(d.classify_preflight({"ready": True})[1], "connected")
        self.assertEqual(d.classify_preflight({"connected": True})[1], "connected")

    def test_bridge_down(self):
        s, mode, _ = d.classify_preflight({"ready": False, "bridgeLinked": False})
        self.assertEqual((s, mode), (d.FAIL, "bridge-down"))

    def test_plugin_not_connected(self):
        s, mode, _ = d.classify_preflight({"ready": False, "bridgeLinked": True, "lastHello": None})
        self.assertEqual((s, mode), (d.FAIL, "plugin-not-connected"))

    def test_transient(self):
        s, mode, _ = d.classify_preflight({"ready": False, "bridgeLinked": True, "lastHello": {"v": 1}})
        self.assertEqual(mode, "transient")


class Report(unittest.TestCase):
    def test_fail_sets_nonzero_exit(self):
        import contextlib
        import io
        findings = [d.finding("x", d.FAIL, "broken", "do y")]
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(d.print_report(findings, as_json=False), 1)
            self.assertEqual(d.print_report([d.finding("x", d.OK, "fine")], as_json=False), 0)
            self.assertEqual(d.print_report([d.finding("x", d.WARN, "meh")], as_json=False), 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
