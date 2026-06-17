#!/usr/bin/env python3
"""Tests for tools/seam_lint.py — deterministic, stdlib-only.

Run: python tests/test_seam_lint.py
"""
import os
import sys
import tempfile
import unittest

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
sys.path.insert(0, os.path.join(ROOT, "tools"))

import seam_lint as sl  # noqa: E402
from pixelpng import write_png, read_png  # noqa: E402

X = (10, 20, 30, 255)
Y = (200, 100, 50, 255)
Z = (0, 255, 0, 255)


def tile_lr(left, right, h=3):
    """A 2-wide tile: column 0 = left colour, column 1 = right colour."""
    px = []
    for _y in range(h):
        px.append(left)
        px.append(right)
    return (2, h, px)


class SeamLint(unittest.TestCase):
    def test_edge_extraction(self):
        w, h, px = tile_lr(X, Y)
        self.assertEqual(sl.edge(px, w, h, "left"), [X, X, X])
        self.assertEqual(sl.edge(px, w, h, "right"), [Y, Y, Y])
        self.assertEqual(sl.edge(px, w, h, "top"), [X, Y])
        self.assertEqual(sl.edge(px, w, h, "bottom"), [X, Y])

    def test_matching_pair_passes(self):
        a = tile_lr(X, Y)
        b = tile_lr(Y, Z)  # b's left column == a's right column
        r = sl.check_pair(a, b, "right")
        self.assertTrue(r["ok"])
        self.assertEqual(r["mismatches"], [])

    def test_mismatched_pair_is_flagged(self):
        a = tile_lr(X, Y)
        c = tile_lr(Z, X)  # c's left column (Z) != a's right column (Y)
        r = sl.check_pair(a, c, "right")
        self.assertFalse(r["ok"])
        self.assertEqual(r["mismatches"], [0, 1, 2])  # every row differs

    def test_strip_of_connecting_tiles_passes(self):
        # 4x3 sheet = [X Y | Y Z], tile size 2x3; tile0.right == tile1.left == Y.
        w, h = 4, 3
        px = []
        for _y in range(h):
            px += [X, Y, Y, Z]
        r = sl.check_strip(w, h, px, 2, 3)
        self.assertTrue(r["ok"], r)
        self.assertEqual((r["cols"], r["rows"]), (2, 1))

    def test_strip_with_a_broken_seam_is_flagged(self):
        # tile1 starts with Z, not Y -> seam between tile0 and tile1 breaks.
        w, h = 4, 3
        px = []
        for _y in range(h):
            px += [X, Y, Z, X]
        r = sl.check_strip(w, h, px, 2, 3)
        self.assertFalse(r["ok"])
        self.assertEqual(len(r["findings"]), 1)
        self.assertEqual(r["findings"][0]["a"], [0, 0])
        self.assertEqual(r["findings"][0]["b"], [1, 0])

    def test_strip_rejects_non_dividing_tile_size(self):
        r = sl.check_strip(5, 3, [X] * 15, 2, 3)  # 5 % 2 != 0
        self.assertFalse(r["ok"])
        self.assertIn("does not divide", r["reason"])

    def test_pair_via_real_pngs(self):
        a = tile_lr(X, Y)
        b = tile_lr(Y, Z)
        with tempfile.TemporaryDirectory() as d:
            pa = os.path.join(d, "a.png")
            pb = os.path.join(d, "b.png")
            write_png(pa, a[0], a[1], a[2])
            write_png(pb, b[0], b[1], b[2])
            r = sl.check_pair(read_png(pa), read_png(pb), "right")
            self.assertTrue(r["ok"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
