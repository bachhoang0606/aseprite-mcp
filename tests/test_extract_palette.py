#!/usr/bin/env python3
"""Tests for tools/extract_palette.py — deterministic, stdlib-only.

Run: python tests/test_extract_palette.py   (exit non-zero on failure; CI-wired)
"""
import json
import os
import sys
import tempfile
import unittest

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
sys.path.insert(0, os.path.join(ROOT, "tools"))

import extract_palette as ep  # noqa: E402
from pixelpng import write_png  # noqa: E402


def luma_of(h):
    r, g, b = int(h[1:3], 16), int(h[3:5], 16), int(h[5:7], 16)
    return 0.299 * r + 0.587 * g + 0.114 * b


class ExtractPalette(unittest.TestCase):
    def test_frequency_returns_exact_limited_palette(self):
        px = [(255, 0, 0, 255), (0, 255, 0, 255), (0, 0, 255, 255), (255, 255, 0, 255)]
        out = ep.extract(px, "frequency", 4)
        self.assertEqual(set(out), {"#FF0000", "#00FF00", "#0000FF", "#FFFF00"})

    def test_frequency_orders_by_count_then_sorts_by_luma(self):
        # red appears 3x, blue 1x; both survive top-2, output is luma-sorted.
        px = [(255, 0, 0, 255)] * 3 + [(0, 0, 255, 255)]
        out = ep.extract(px, "frequency", 2)
        self.assertEqual(out, ["#0000FF", "#FF0000"])  # blue luma < red luma

    def test_transparent_pixels_are_ignored(self):
        px = [(255, 0, 0, 255), (10, 20, 30, 0)]  # second is alpha-0 junk
        self.assertEqual(ep.extract(px, "frequency", 8), ["#FF0000"])

    def test_requested_more_than_distinct_returns_all_distinct(self):
        px = [(0, 0, 0, 255), (255, 255, 255, 255)]
        out = ep.extract(px, "frequency", 16)
        self.assertEqual(out, ["#000000", "#FFFFFF"])  # luma-sorted

    def test_output_is_luma_sorted(self):
        px = [(255, 255, 255, 255), (0, 0, 0, 255), (128, 128, 128, 255)]
        out = ep.extract(px, "frequency", 3)
        lumas = [luma_of(h) for h in out]
        self.assertEqual(lumas, sorted(lumas))

    def test_median_cut_splits_into_two_clusters(self):
        px = [(0, 0, 0, 255), (10, 5, 2, 255), (245, 250, 255, 255), (255, 255, 250, 255)]
        out = ep.extract(px, "median_cut", 2)
        self.assertEqual(len(out), 2)
        self.assertLess(luma_of(out[0]), 80)     # a dark cluster
        self.assertGreater(luma_of(out[1]), 180)  # a light cluster

    def test_kmeans_is_deterministic(self):
        px = [(0, 0, 0, 255), (8, 8, 8, 255), (250, 250, 250, 255), (255, 255, 255, 255)]
        a = ep.extract(px, "kmeans", 2)
        b = ep.extract(px, "kmeans", 2)
        self.assertEqual(a, b)
        self.assertEqual(len(a), 2)

    def test_empty_or_fully_transparent_input(self):
        self.assertEqual(ep.extract([], "frequency", 4), [])
        self.assertEqual(ep.extract([(1, 2, 3, 0)], "median_cut", 4), [])

    def test_reads_a_real_png_and_saves_compatible_palette(self):
        # write a real 2x2 PNG, extract from disk, and save a lint-compatible JSON.
        from pixelpng import read_png
        px = [(255, 0, 0, 255), (0, 255, 0, 255), (0, 0, 255, 255), (16, 16, 16, 255)]
        with tempfile.TemporaryDirectory() as d:
            png = os.path.join(d, "ref.png")
            write_png(png, 2, 2, px)
            w, h, read = read_png(png)
            self.assertEqual((w, h), (2, 2))
            out = ep.extract(read, "frequency", 4)
            self.assertEqual(set(out), {"#FF0000", "#00FF00", "#0000FF", "#101010"})
            # the saved shape must match what lint_sprite.load_palette expects.
            pal = os.path.join(d, "pal.json")
            with open(pal, "w", encoding="utf-8") as f:
                json.dump({"name": "x", "source": "y", "colors": out}, f)
            with open(pal, encoding="utf-8") as f:
                loaded = json.load(f)
            self.assertIn("colors", loaded)
            self.assertTrue(all(c.startswith("#") and len(c) == 7 for c in loaded["colors"]))


if __name__ == "__main__":
    unittest.main(verbosity=2)
