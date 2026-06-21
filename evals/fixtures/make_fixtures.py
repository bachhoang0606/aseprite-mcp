#!/usr/bin/env python3
"""Generate the deterministic golden animation fixtures for the silhouette-IoU drift
gate (SPEC-007 Phase 1, checklist 9.3/9.4). The committed PNGs are a **snapshot
contract** — regenerate with this script; a change is an intentional, reviewed commit
(like any visual-regression baseline), never worked around.

  python evals/fixtures/make_fixtures.py

Writes:
  walk_stable.png — a clean 4-frame walk (8×16 body, 1px vertical bob): min
                    adjacent-frame silhouette IoU ~0.88, above the 0.80 floor.
  walk_drift.png  — same, but frame 2's body balloons to 16×16 (proportion drift):
                    min IoU ~0.45, below the floor → the gate must catch it.
Both are horizontal strips of 4 frames, frame_width = 24, no gap.
"""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "tools"))
from pixelpng import write_png  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
FW = FH = 24
GREEN = (72, 156, 76, 255)
T = (0, 0, 0, 0)


def strip(rects):
    """rects: per-frame body rect (x0, y0, w, h). Returns a (len*FW)×FH strip."""
    width = FW * len(rects)
    px = [T] * (width * FH)
    for k, (rx, ry, rw, rh) in enumerate(rects):
        ox = k * FW
        for y in range(ry, ry + rh):
            for x in range(rx, rx + rw):
                if 0 <= x < FW and 0 <= y < FH:
                    px[y * width + ox + x] = GREEN
    return width, FH, px


def main():
    # Stable: same 8×16 body, 1px vertical bob → high inter-frame overlap.
    stable = [(8, 4, 8, 16), (8, 3, 8, 16), (8, 4, 8, 16), (8, 3, 8, 16)]
    w, h, px = strip(stable)
    write_png(os.path.join(HERE, "walk_stable.png"), w, h, px)
    # Drift: frame 2's body balloons to 16×16 (proportion drift) → low overlap.
    drift = [(8, 4, 8, 16), (8, 3, 8, 16), (4, 4, 16, 16), (8, 4, 8, 16)]
    w, h, px = strip(drift)
    write_png(os.path.join(HERE, "walk_drift.png"), w, h, px)
    print("wrote walk_stable.png + walk_drift.png (frame_width=24, 4 frames)")


if __name__ == "__main__":
    main()
