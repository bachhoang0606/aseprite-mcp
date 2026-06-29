# Demo reel — social copy & honesty note

The reel (`out/reel_45s.gif`, and a tighter `out/reel_30s.gif` for feeds) is the
README hero and the X/Reddit clip. It is **assembled programmatically** by
`reel.py` — no screen recording, no hand-faked mockup.

## What's real vs. staged (read this before posting)

- **Real:** every animated pixel is produced by the same op-rasterizer + locked
  palette the tool enforces (`pixels.py`); every framed "receipt" thumbnail is a
  PNG actually emitted in a live Aseprite session under `evals/runs/`
  (`cand_A`/`cand_B` = the real flat→hue-shifted goblin pass; `v_anim_idle` = the
  real knight idle filmstrip; `v_autotile` = a real blob-47 autotile sheet;
  `v_dither_gradient` / `v_rotate` = real palette-legal ops). The three numbers
  (+1.33 perception, 0 off-palette, +4.0 reference) are the project's
  blind-judged benchmark results.
- **Staged for clarity:** the hero goblin and its flaw→fix beat are *re-created*
  from the project's logic (locked 12-colour palette, hue-shift shading rule, the
  real perception methodology of catching a mismatched-eye pass) and paced into a
  clean 45s story. The mismatched-eye "self-critique" is dramatised on the staged
  hero; the *receipt* beside it (`cand_A → cand_B`) is the real before/after.
- The end card carries this verbatim: **"Real tool outputs · Staged for clarity."**
  Keep that line. It's the honesty contract.

Scaling is **nearest-neighbor only** — never blur pixel art.

## X / Twitter

> I gave an AI my open Aseprite file. It started drawing —
> locked a palette, blocked the shape, hue-shifted the shading…
> then **looked at its own work, caught a mismatched eye, and fixed it.**
>
> Live in your editor. Measured, not vibes (blind-judged).
> github.com/bachhoang0606/aseprite-mcp

Alt hook (catch-its-own-mistakes angle):

> An AI pixel-artist that catches its own mistakes. Watch it spot a bad eye and
> fix it — then animate, autotile, and export engine-ready art, live in Aseprite.

## Reddit (r/aseprite, r/gamedev, r/proceduralgeneration) title

> I built an MCP server so an AI agent can draw pixel art *live* in my Aseprite —
> and review & fix its own work. Open-source, blind-judged. [OC reel]

## README alt-text (already embedded)

> Demo reel: an AI agent draws a goblin sprite live in Aseprite — locks a palette,
> blocks the silhouette, applies hue-shifted shading, then reviews its own work,
> catches a mismatched eye and fixes it, animates a walk cycle, autotiles a scene
> and exports an engine-ready sheet. Built from real tool outputs and blind-judged
> numbers; staged for clarity.

## Rebuilding

```bash
python demo/reel/reel.py --cut 45     # README hero  → out/reel_45s.gif (<3 MB)
python demo/reel/reel.py --cut 30     # tighter feed cut → out/reel_30s.gif
python demo/reel/reel.py --cut 45 --frames   # also dump PNG frames (gitignored)
```

For an MP4 (X/Reddit prefer video), render frames then:

```bash
ffmpeg -framerate 12 -i demo/reel/out/frames/f%04d.png \
  -vf "scale=1080:1080:flags=neighbor" -c:v libx264 -pix_fmt yuv420p \
  demo/reel/out/reel_45s.mp4
```

(`ffmpeg` is not required to build the GIF; it's only for the optional MP4.)
