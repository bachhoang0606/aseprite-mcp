# Tool-usage correctness — run 2026-06-24

8 usage tasks across 6 candidate tools, scored under **full** vs **trimmed** tool-level
description. The agent saw the param signature (constant) + the description (the variable),
**no param-level docs** (conservative / worst-case for the trim).

## Result
| condition | usage_accuracy | param_recall |
|---|--:|--:|
| **full** | **1.000** | 1.000 |
| **trimmed** | **1.000** | 1.000 |

- **usage Δ (trimmed − full) = 0.0** → trimming these 6 descriptions did **not** degrade correct
  usage on this suite.
- **description tokens: 1,486 → 300 (saving ≈ 1,186)** when those schemas are loaded.
- **`trim_is_safe = true`.**

## The interesting part — it corrected a wrong assumption
The `preview_invert` case was designed to catch a load-bearing trim: `save_preview`'s full
description contains the gutter-inversion formula `source_x = (preview_x − left_w)/scale`, which the
trimmed version drops. Hypothesis: trimming it would break the compute case.

**It didn't.** Under the *trimmed* description the model still computed the correct source pixel
`(16, 8)` — it **re-derived** the inversion from first principles (one agent even cited
`src/gutter.rs` line numbers). So that formula is **not load-bearing for a capable model**: keeping a
2.3k-char description just to restate a standard "subtract the gutter band, divide by scale" is not
buying correctness. The harness measured the assumption and refuted it — exactly its purpose.

## Verdict
**Trimming the 6 oversized descriptions is measured-safe** (no usage regression) and saves ~1,186
tokens whenever those schemas are loaded (every turn for eager clients; per-use for deferred clients,
and `save_preview` loads often in the perception loop). → Worth applying.

## Caveats (honest)
- The model **re-derived** the inversion partly with repo knowledge (it cited `gutter.rs`); a weaker
  model or a cold context might lean on the description more. The safe call is to keep the *essential*
  cue ("invert gutter coords by subtracting the band and dividing by scale") in the trimmed text even
  while dropping the long worked example — i.e. trim verbosity, not the one load-bearing sentence.
- N=8 tasks, single model, 1-shot. The test withheld param-level docs (worst-case), so the real
  safety margin is *better* than measured. Re-run with more tasks before trusting beyond these 6 tools.
