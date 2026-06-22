#!/usr/bin/env python3
"""Tests for tools/asset_search.py — deterministic, stdlib-only, OFFLINE (no network).

Run: python tests/test_asset_search.py   (exit non-zero on failure; CI-wired)
"""
import json
import os
import sys
import tempfile
import unittest

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
sys.path.insert(0, os.path.join(ROOT, "tools"))

import asset_search as a  # noqa: E402


class Parsers(unittest.TestCase):
    def test_lospec_normalizes_hex_and_carries_meta(self):
        c = a.parse_lospec_palette({"name": "Demo", "author": "Artist", "colors": ["ffffff", "#0a0B0c"]}, "demo")
        self.assertEqual(c["colors"], ["#FFFFFF", "#0A0B0C"])
        self.assertEqual((c["author"], c["name"], c["kind"]), ("Artist", "Demo", "palette"))
        self.assertTrue(c["source_url"].endswith("/palette-list/demo"))
        self.assertEqual(c["license"], a.PALETTE_LICENSE)

    def test_lospec_falls_back_to_slug_when_unnamed(self):
        c = a.parse_lospec_palette({"colors": []}, "slugonly")
        self.assertEqual(c["name"], "slugonly")
        self.assertEqual(c["author"], "unknown")

    def test_hf_keeps_good_skips_junk_and_defaults_cc0(self):
        out = a.parse_hf_rows({"rows": [
            {"row": {"id": 1, "title": "Tree", "image": {"src": "https://x/t.png"}, "tags": ["nature"]}},
            {"row": {"junk": True}},     # no name/id -> skipped
            "not-a-dict",                # malformed -> skipped
        ]})
        self.assertEqual(len(out), 1)
        self.assertEqual(out[0]["name"], "Tree")
        self.assertEqual(out[0]["license"], "CC0-1.0")
        self.assertEqual(out[0]["download_url"], "https://x/t.png")
        self.assertEqual(out[0]["tags"], ["nature"])

    def test_hf_respects_explicit_license_and_url_fields(self):
        out = a.parse_hf_rows({"rows": [
            {"row": {"title": "Sword", "license": "CC-BY-3.0",
                     "download_url": "https://x/s.png", "thumbnail": "https://x/s_thumb.png",
                     "tags": "weapon;metal"}},
        ]})
        self.assertEqual(out[0]["license"], "CC-BY-3.0")
        self.assertEqual(out[0]["preview_url"], "https://x/s_thumb.png")
        self.assertEqual(out[0]["tags"], ["weapon", "metal"])

    def test_hf_limit(self):
        rows = [{"row": {"id": i, "title": f"a{i}"}} for i in range(10)]
        self.assertEqual(len(a.parse_hf_rows({"rows": rows}, limit=3)), 3)


class Gate(unittest.TestCase):
    def test_truthy(self):
        for v in ["1", "true", "YES", "On"]:
            self.assertTrue(a.is_truthy(v))
        for v in ["0", "no", "", "maybe"]:
            self.assertFalse(a.is_truthy(v))

    def test_network_allowed_flag_and_env(self):
        self.assertTrue(a.network_allowed(True))
        old = os.environ.pop(a.ALLOW_NET_ENV, None)
        try:
            self.assertFalse(a.network_allowed(False))
            os.environ[a.ALLOW_NET_ENV] = "1"
            self.assertTrue(a.network_allowed(False))
        finally:
            os.environ.pop(a.ALLOW_NET_ENV, None)
            if old is not None:
                os.environ[a.ALLOW_NET_ENV] = old

    def test_gate_error_names_the_flag(self):
        self.assertIn(a.ALLOW_NET_ENV, a.gate_error("x"))
        self.assertIn("--online", a.gate_error("x"))


class Guards(unittest.TestCase):
    def test_url_scheme_guard(self):
        for bad in ["file:///etc/passwd", "ftp://x/y", "javascript:alert(1)", "data:text/plain,hi"]:
            with self.assertRaises(ValueError):
                a._require_http_url(bad)
        for ok in ["http://x/y", "https://x/y"]:
            self.assertEqual(a._require_http_url(ok), ok)

    def test_safe_name_blocks_traversal(self):
        self.assertEqual(a._safe_name("../../etc/passwd"), "etc-passwd")
        self.assertEqual(a._safe_name("a/b\\c"), "a-b-c")
        self.assertEqual(a._safe_name(""), "asset")

    def test_ext_from_url(self):
        self.assertEqual(a._ext_from_url("https://x/y.PNG"), ".png")
        self.assertEqual(a._ext_from_url("https://x/y.jpg"), ".jpg")
        self.assertEqual(a._ext_from_url("https://x/y"), ".png")        # unknown -> png
        self.assertEqual(a._ext_from_url("https://x/y.exe"), ".png")    # non-image -> png


class CatalogSearch(unittest.TestCase):
    def setUp(self):
        self.catalog = a.load_catalog(a.DEFAULT_CATALOG)

    def test_local_palette_resolves_colors_offline(self):
        pico = [c for c in a.search_catalog(self.catalog, ROOT) if c["id"] == "pico-8"]
        self.assertTrue(pico and pico[0]["colors"], "pico-8 colours load from its local file")
        self.assertTrue(all(col.startswith("#") for col in pico[0]["colors"]))

    def test_lospec_pointer_needs_online(self):
        e = [c for c in a.search_catalog(self.catalog, ROOT) if c["id"] == "endesga-32"][0]
        self.assertIsNone(e["colors"])
        self.assertTrue(e["needs_online"])

    def test_source_and_query_and_tag_filters(self):
        self.assertTrue(all(c["source"] == "hf" for c in a.search_catalog(self.catalog, ROOT, source="hf")))
        self.assertTrue(a.search_catalog(self.catalog, ROOT, query="pico"))
        self.assertEqual(a.search_catalog(self.catalog, ROOT, query="no-such-thing-xyz"), [])
        tagged = a.search_catalog(self.catalog, ROOT, tag="retro")
        self.assertTrue(tagged and all("retro" in c["tags"] for c in tagged))

    def test_sorted_and_limited(self):
        res = a.search_catalog(self.catalog, ROOT, limit=2)
        self.assertEqual(len(res), 2)
        keys = [(c["source"], c["id"]) for c in res]
        self.assertEqual(keys, sorted(keys))


class Provenance(unittest.TestCase):
    def test_credits_line_has_all_fields(self):
        rec = {"name": "X", "author": "Y", "license": "L", "source_url": "https://u", "file": "f.json"}
        line = a.credits_line(rec)
        for piece in ["X", "Y", "L", "https://u", "f.json"]:
            self.assertIn(piece, line)

    def test_manifest_record_includes_palette_colors_only(self):
        pal = {"kind": "palette", "source": "local", "id": "p", "name": "P", "author": "A",
               "license": "L", "source_url": "u", "colors": ["#000000"]}
        self.assertEqual(a.manifest_record(pal, "p.json", True)["colors"], ["#000000"])
        asset = {**pal, "kind": "asset", "colors": None}
        self.assertNotIn("colors", a.manifest_record(asset, "a.png", False))

    def test_append_manifest_and_credits_accumulate(self):
        with tempfile.TemporaryDirectory() as d:
            r1 = {"kind": "palette", "source": "local", "id": "a", "name": "A", "author": "x",
                  "license": "L", "source_url": "u", "file": "a.json"}
            a.append_manifest(d, r1)
            a.append_manifest(d, {**r1, "id": "b", "file": "b.json"})
            with open(os.path.join(d, "MANIFEST.json"), encoding="utf-8") as fh:
                man = json.load(fh)
            self.assertEqual([m["id"] for m in man], ["a", "b"])
            a.append_credits(d, r1)
            with open(os.path.join(d, "CREDITS.txt"), encoding="utf-8") as fh:
                txt = fh.read()
            self.assertIn("# Asset credits", txt)


class Fetch(unittest.TestCase):
    def setUp(self):
        self.catalog = a.load_catalog(a.DEFAULT_CATALOG)
        self.cands = a.search_catalog(self.catalog, ROOT)

    def _by_id(self, i):
        return [c for c in self.cands if c["id"] == i][0]

    def test_fetch_local_palette_offline_writes_everything(self):
        with tempfile.TemporaryDirectory() as d:
            rec = a.fetch_candidate(self._by_id("pico-8"), d, online=False)
            self.assertTrue(rec["fetched_offline"])
            self.assertNotIn("CC0", rec["license"])  # palette ≠ CC0 claim
            with open(os.path.join(d, "pico-8.json"), encoding="utf-8") as fh:
                pal = json.load(fh)
            self.assertIn("colors", pal)
            self.assertTrue(os.path.exists(os.path.join(d, "MANIFEST.json")))
            self.assertTrue(os.path.exists(os.path.join(d, "CREDITS.txt")))

    def test_fetch_online_only_palette_blocks_offline(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(PermissionError):
                a.fetch_candidate(self._by_id("endesga-32"), d, online=False)

    def test_fetch_asset_blocks_offline(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(PermissionError):
                a.fetch_candidate(self._by_id("opengameart-cc0"), d, online=False)

    def test_no_network_imports(self):
        # Lean-deps / SAC: the module must be stdlib-only (no requests/duckdb/pyarrow).
        with open(os.path.join(ROOT, "tools", "asset_search.py"), encoding="utf-8") as fh:
            src = fh.read()
        for banned in ["import requests", "import duckdb", "import pyarrow", "from requests"]:
            self.assertNotIn(banned, src)


class Cli(unittest.TestCase):
    """CLI-level checks for the --url fetch path (no network: gate + scheme guard only)."""

    @staticmethod
    def _run(argv):
        import contextlib
        import io
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            return a.main(argv)

    def test_url_fetch_blocked_offline(self):
        old = os.environ.pop(a.ALLOW_NET_ENV, None)
        try:
            self.assertEqual(self._run(["fetch", "--url", "https://x/y.png"]), 3)
        finally:
            if old is not None:
                os.environ[a.ALLOW_NET_ENV] = old

    def test_url_fetch_rejects_bad_scheme_even_online(self):
        self.assertEqual(self._run(["fetch", "--url", "file:///etc/passwd", "--online"]), 2)

    def test_fetch_needs_id_or_url(self):
        self.assertEqual(self._run(["fetch"]), 2)

    def test_candidate_from_url_carries_provenance(self):
        c = a.candidate_from_url("https://x/tree.png", name="Tree", author="Bob", license_="CC0-1.0", source="hf")
        self.assertEqual(
            (c["kind"], c["name"], c["author"], c["license"], c["download_url"], c["id"]),
            ("asset", "Tree", "Bob", "CC0-1.0", "https://x/tree.png", "tree.png"),
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
