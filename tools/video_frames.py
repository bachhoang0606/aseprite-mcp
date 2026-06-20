#!/usr/bin/env python3
"""Extract motion key-frames from a video/GIF and chroma-key the background out,
for the /pixel-reference-motion skill (research doc §C1, roadmap #7).

Generating each animation frame independently drifts the character every frame; a
single consistent reference motion (a 4-second green-screen clip, an animated GIF, or
a hand-supplied PNG sequence) anchors all of them. This turns that source into a clean
PNG sequence the agent then imports (live_import_reference, SPEC-006) onto a per-frame
reference layer and traces over.

Two independent stages:
  1. EXTRACT  K evenly-spaced frames from a video/GIF via ffmpeg (must be on PATH).
  2. CHROMA-KEY a solid background (default green #00ff00) to transparent, using the
     adaptive green-dominance test  g - max(r,b) > threshold  from Mike Veerman's
     "Claude After Dark" pipeline. A non-green backdrop uses max-channel distance to
     --key-color instead. Thresholds are CLI-configurable (tune per clip).

Stdlib-only — reuses tools/pixelpng.py for PNG read/write. ffmpeg is the one external
dependency and ONLY the extract stage needs it: point --frames at an existing PNG dir
to skip extraction entirely. ffprobe (ships with ffmpeg) is used to count frames for
--count; pass --fps instead if it is unavailable.

Usage:
  # video -> 6 evenly-spaced, green-keyed frames in C:/tmp/ref
  python tools/video_frames.py run.mp4 --out C:/tmp/ref --count 6
  # explicit sampling rate instead of a count; looser key threshold
  python tools/video_frames.py run.mp4 --out C:/tmp/ref --fps 1.5 --chroma-threshold 30
  # key a blue screen by colour distance
  python tools/video_frames.py run.mp4 --out C:/tmp/ref --count 6 --key-color #1030ff
  # chroma-key an existing PNG sequence in place (no ffmpeg needed)
  python tools/video_frames.py --frames C:/tmp/ref --out C:/tmp/ref
  # extract only, keep the background
  python tools/video_frames.py run.mp4 --out C:/tmp/ref --count 6 --no-chroma
  # verify the pure colour-keying logic
  python tools/video_frames.py --selftest

Prints a JSON report (frames written + keyed-pixel ratio per frame); exit 1 on error.
"""
import argparse
import glob
import json
import os
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pixelpng import read_png, write_png  # noqa: E402

GREEN = (0, 255, 0)
# Default thresholds differ by mode: green-dominance is a *dominance* margin (higher =
# stricter), colour-distance is a max-channel distance (higher = looser).
DEFAULT_DOMINANCE = 20
DEFAULT_DISTANCE = 40


def green_dominance(r, g, b):
    """How strongly green a pixel is: g minus the larger of its red/blue. A green
    screen scores high; skin/cloth score low or negative. (Mike Veerman, §C1.)"""
    return g - max(r, b)


def key_pixel(px, mode, threshold, key_rgb):
    """Return px with alpha forced to 0 if it is background, else unchanged.

    mode 'dominance': background == green_dominance(px) > threshold.
    mode 'distance' : background == max per-channel distance to key_rgb <= threshold.
    Already-transparent pixels pass through untouched.
    """
    r, g, b, a = px
    if a == 0:
        return px, False
    if mode == "dominance":
        is_bg = green_dominance(r, g, b) > threshold
    else:
        kr, kg, kb = key_rgb
        is_bg = max(abs(r - kr), abs(g - kg), abs(b - kb)) <= threshold
    if is_bg:
        return (r, g, b, 0), True
    return px, False


def key_image(pixels, mode, threshold, key_rgb):
    """Chroma-key a flat (r,g,b,a) list; return (new_pixels, keyed_count)."""
    out = []
    keyed = 0
    for px in pixels:
        new_px, was_keyed = key_pixel(px, mode, threshold, key_rgb)
        out.append(new_px)
        if was_keyed:
            keyed += 1
    return out, keyed


def parse_hex(s):
    s = s.lstrip("#")
    if len(s) != 6:
        raise ValueError(f"bad --key-color {s!r}: expected #rrggbb")
    return tuple(int(s[i : i + 2], 16) for i in (0, 2, 4))


def _which(name, override):
    if override:
        return override if os.path.exists(override) else None
    return shutil.which(name)


def probe_duration(ffprobe, src):
    """Seconds of `src` via ffprobe, or None if it can't be determined."""
    try:
        out = subprocess.run(
            [ffprobe, "-v", "error", "-show_entries", "format=duration",
             "-of", "default=noprint_wrappers=1:nokey=1", src],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        return float(out)
    except (subprocess.CalledProcessError, ValueError, OSError):
        return None


def extract(src, out_dir, count, fps, ffmpeg, ffprobe):
    """Run ffmpeg to write out_dir/NNN.png; return the sorted list of written paths."""
    if fps is None:
        # Derive an fps that yields ~count evenly-spaced frames across the clip.
        dur = probe_duration(ffprobe, src) if ffprobe else None
        if dur is None or dur <= 0:
            raise SystemExit(
                "could not probe duration for --count (ffprobe missing or unreadable clip); "
                "pass --fps N instead (e.g. --fps 1.5)"
            )
        fps = max(count / dur, 1e-6)
    pattern = os.path.join(out_dir, "%03d.png")
    cmd = [ffmpeg, "-y", "-i", src, "-vf", f"fps={fps}"]
    if count is not None:
        cmd += ["-frames:v", str(count)]
    cmd += [pattern]
    try:
        subprocess.run(cmd, capture_output=True, text=True, check=True)
    except subprocess.CalledProcessError as e:
        raise SystemExit(f"ffmpeg failed:\n{e.stderr.strip()[-800:]}")
    return sorted(glob.glob(os.path.join(out_dir, "*.png")))


def selftest():
    # green-dominance: a green-screen pixel keys, skin/cloth does not.
    assert key_pixel((10, 230, 12, 255), "dominance", 20, GREEN)[1] is True
    assert key_pixel((200, 150, 120, 255), "dominance", 20, GREEN)[1] is False  # skin
    assert key_pixel((0, 60, 0, 255), "dominance", 20, GREEN)[1] is True  # dark green
    assert key_pixel((40, 50, 40, 255), "dominance", 20, GREEN)[1] is False  # near-gray
    # already-transparent pixels pass through, never recounted.
    assert key_pixel((0, 255, 0, 0), "dominance", 20, GREEN) == ((0, 255, 0, 0), False)
    # distance mode keys near the key colour and spares far colours.
    assert key_pixel((18, 48, 250, 255), "distance", 40, (16, 48, 255))[1] is True
    assert key_pixel((200, 40, 40, 255), "distance", 40, (16, 48, 255))[1] is False
    # whole-image count is exact.
    px = [(0, 255, 0, 255), (200, 150, 120, 255), (5, 240, 5, 255)]
    _, keyed = key_image(px, "dominance", 20, GREEN)
    assert keyed == 2, keyed
    print(json.dumps({"selftest": "ok"}))


def main(argv=None):
    ap = argparse.ArgumentParser(description="Extract + chroma-key motion reference frames.")
    ap.add_argument("source", nargs="?", help="input video or GIF (omit with --frames)")
    ap.add_argument("--frames", help="use an existing dir of PNGs instead of extracting")
    ap.add_argument("--out", help="output dir for the PNG sequence")
    ap.add_argument("--count", type=int, help="number of evenly-spaced frames to extract")
    ap.add_argument("--fps", type=float, help="ffmpeg sampling rate (overrides --count math)")
    ap.add_argument("--key-color", default="#00ff00", help="background colour (default green)")
    ap.add_argument("--chroma-threshold", type=int, default=None,
                    help="dominance margin (green) or max-channel distance (other colour)")
    ap.add_argument("--no-chroma", action="store_true", help="keep the background")
    ap.add_argument("--ffmpeg", help="path to ffmpeg (default: PATH)")
    ap.add_argument("--ffprobe", help="path to ffprobe (default: PATH)")
    ap.add_argument("--selftest", action="store_true", help="run pure-logic asserts and exit")
    args = ap.parse_args(argv)

    if args.selftest:
        selftest()
        return 0

    if not args.out:
        ap.error("--out is required")
    os.makedirs(args.out, exist_ok=True)

    try:
        key_rgb = parse_hex(args.key_color)
    except ValueError as e:
        raise SystemExit(str(e))
    mode = "dominance" if key_rgb == GREEN else "distance"
    threshold = args.chroma_threshold
    if threshold is None:
        threshold = DEFAULT_DOMINANCE if mode == "dominance" else DEFAULT_DISTANCE

    # 1. Gather the source PNGs (extract, or reuse an existing dir).
    if args.frames:
        frames = sorted(glob.glob(os.path.join(args.frames, "*.png")))
        if not frames:
            raise SystemExit(f"no PNGs in --frames dir {args.frames!r}")
    else:
        if not args.source:
            ap.error("give a source video/GIF, or --frames DIR")
        ffmpeg = _which("ffmpeg", args.ffmpeg)
        if not ffmpeg:
            raise SystemExit(
                "ffmpeg not found on PATH — install it, pass --ffmpeg PATH, or supply an "
                "already-extracted PNG sequence with --frames DIR"
            )
        ffprobe = _which("ffprobe", args.ffprobe)
        frames = extract(args.source, args.out, args.count, args.fps, ffmpeg, ffprobe)
        if not frames:
            raise SystemExit("ffmpeg wrote no frames (check the clip and --fps/--count)")

    # 2. Chroma-key each frame (in place into --out, preserving file order).
    report = []
    for i, src_path in enumerate(frames, 1):
        w, h, pixels = read_png(src_path)
        if args.no_chroma:
            keyed, total = 0, w * h
            out_pixels = pixels
        else:
            out_pixels, keyed = key_image(pixels, mode, threshold, key_rgb)
            total = w * h
        dst = os.path.join(args.out, f"{i:03d}.png")
        write_png(dst, w, h, out_pixels)
        report.append({
            "frame": i, "path": dst.replace("\\", "/"), "size": [w, h],
            "keyed_ratio": round(keyed / total, 4) if total else 0.0,
        })

    print(json.dumps({
        "frames": len(report),
        "out": args.out.replace("\\", "/"),
        "chroma": None if args.no_chroma else {"mode": mode, "threshold": threshold,
                                               "key_color": args.key_color},
        "report": report,
    }, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
