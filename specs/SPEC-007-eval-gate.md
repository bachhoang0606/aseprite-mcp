# SPEC-007 — Objective eval harness as a hard gate (roadmap #9)

- Status: **Draft (2026-06-21)** — design only; implementation is a follow-up.
- Owner: project
- Checklist items advanced: **9.4** (eval harness for skills/agents → 3), **9.3**
  (visual-regression strengthened), supports 9.1/9.2.
- Related ADRs: extends the Tier-A / Tier-B split documented in
  [`evals/README.md`](../evals/README.md); no new return-type / protocol contract → no
  new ADR. The "evals are a **blocking** CI gate" choice is recorded below under Decisions.
- Source: research doc [`docs/research/agent-pixel-art-techniques.md`](../docs/research/agent-pixel-art-techniques.md)
  Path 6 (§F, "Objective validation harness"), §A (donut test / SwordsBench / cross-frame
  drift / animation review), roadmap **#9**, and the "Open questions" (no live capability
  benchmark of *our* stack; the "artistic agent" persona is unmeasured).

## Intent
The research's load-bearing finding is that an agent will **congratulate itself on
objectively-broken art** as context fills (the *donut test*, §A) — so the one thing that
makes every other capability trustworthy is an **objective validator in the loop, used as a
hard gate** (§F: "the only antidote… but it must be a *hard gate*"). The repo already has
the machinery — `evals/run.py` (Tier-A, deterministic, in `quality.yml`) and `evals/judge.py`
+ `evals/tier_b.json` (Tier-B, LLM-judged, manual) — but it is missing the four pieces
roadmap #9 names (**SwordsBench tasks, silhouette-IoU drift metric, long-session
degradation, persona A/B**), and it does not yet answer the open question *"does our
perception / constrained-colour stack actually move output on this server?"*. SPEC-007 adds
those and makes the deterministic layer a **blocking CI gate**.

## Inputs / Outputs
- **Inputs:** rendered frame / sprite PNGs (golden fixtures under `evals/fixtures/`, or live
  `live_save_filmstrip` / `live_save_preview` output); the existing `tier_b.json` rubric
  schema (`{id, component, checklist, prompt, pass_threshold, rubric:[{id, weight, must_pass?,
  rule, desc}]}`); capability toggles (perception preview on/off, constrained-colour snap
  on/off) for the cross-path benchmark; the candidate "artistic agent" persona line.
- **Outputs:** new **deterministic Tier-A checks** (exit-non-zero → CI gate); new **Tier-B
  case definitions** (CI-validated for well-formedness); recorded benchmark / degradation /
  A-B results in `evals/RESULTS.md` (+ a new `evals/BENCHMARK.md`), each re-derivable from
  archived evidence under `evals/runs/<YYYY-MM-DD>/`. All metric math is **pure Python**
  (stdlib, reuses `tools/pixelpng.py`) → unit-testable, no Aseprite, **no new dependency**.

## Behaviour

### Phase 1 — the deterministic hard gate (Tier-A, in CI)
1. **`silhouette_iou` drift metric** — pure Python in `tools/silhouette_iou.py` (reuses
   `pixelpng.read_png`). A frame's *silhouette mask* is its set of non-transparent pixels;
   `iou(a, b) = |A∩B| / |A∪B|`. For an animation (a film-strip PNG sliced into a tile grid,
   or an explicit frame-PNG list) report each **adjacent-pair IoU + the minimum**. High IoU
   between consecutive *walk/idle* frames = body volume preserved; a sudden low IoU = the
   cross-frame **proportion drift** SwordsBench names as the #1 animation failure (§A). A
   **per-case config lives in `cases.json`** with the fixture: `min_iou` (drift floor,
   default `0.80`) and `high_motion: true` for lunge/attack tags — which *should* move a lot,
   so for those the check compares **silhouette bbox-area stability** (volume preserved)
   rather than raw mask IoU, with a looser `min_iou` (default `0.55`).
2. **Golden fixtures + CI checks** — add `silhouette_iou_stable` (a clean golden animation
   stays above the drift floor) and `silhouette_iou_detects_drift` (a deliberately-warped
   golden trips it) to `evals/run.py::CHECKS`, mapped in `cases.json`. `quality.yml` already
   fails the job on a non-zero `run.py`, so registering these makes drift a **hard gate** with
   no workflow change.
3. **SwordsBench case definitions** — add the verbatim SwordsBench prompts as `tier_b.json`
   cases (`tb_swords_static`, `tb_swords_walk`) with weighted rubrics tied to checklist 5.x +
   `rules/` (silhouette readability, palette discipline, cross-frame drift). The existing
   `tier_b_cases_wellformed` Tier-A check validates them (weights sum to 1.0, cited rule files
   exist, checklist ids real) so they cannot silently rot — even before any live run.

### Phase 2 — the live measurement (Tier-B, manual / on-demand, recorded)
4. **Long-session degradation (donut test)** — a documented protocol + a `judge.py` helper:
   run a long multi-step task and snapshot an **objective** quality vector (linter pass-rate
   via `tools/lint_sprite.py` + `silhouette_iou` + off-palette count) at N context-fill
   checkpoints (e.g. 0 / 20 / 40 / 60 % of a token budget). The three metrics are tracked
   **separately** (not blended into one number): linter pass-rate (0–1), min `silhouette_iou`,
   off-palette pixel count. **Regression = any checkpoint dropping below its 0%-baseline by
   more than the margin** (linter −10%, min IoU below the case floor, or off-palette > 0 when
   the baseline was 0). One recorded run in `RESULTS.md` (row: date, build, task, the
   per-checkpoint vectors, the slope, pass/fail). This turns the donut test from an anecdote
   into a repeatable measurement.
5. **Persona A/B** — the **persona line** is a single candidate sentence (e.g. an
   "you are a meticulous pixel artist who values silhouette and palette discipline" framing)
   added to a skill/agent system prompt. `judge.py --emit-ab <case>` emits a **paired**
   (with-persona / without-persona) judge prompt for the same SwordsBench task; run both, judge
   **blind** (the judge does not know which is which), record the delta. Because the §D caveat
   is that this is the source's *untested hypothesis*, the line is **adopted only if ≥3 blind
   A/B runs show a mean Δscore ≥ +0.05 with a consistent sign** (signal, not noise) — and then
   it is wired into the relevant `skills/`/agent prompt; otherwise it stays out.
6. **Cross-path capability benchmark** — run the SwordsBench tasks through Tier-B with each
   capability path toggled, recording the per-path score delta in `evals/BENCHMARK.md` with
   archived evidence. Two paths, toggled as **manual run-variants** (no code flags — the
   executor includes or omits a step): **Path 1 perception** = run the draw→see→fix loop *with*
   vs *without* the `live_save_preview` see-step; **Path 2 constrained-colour** = draw *with*
   vs *without* `live_palette_snap` / `clamp_to_palette`. (A live Tier-B run, not offline.)
   This answers the open question "how much does Path 1 actually move output on *this* server"
   with numbers, not belief.

### Decisions
1. **Reuse the existing two-tier split** (`evals/README.md`): deterministic → Tier-A → CI hard
   gate; live LLM-judged → Tier-B → manual but CI-validated for well-formedness. Do not
   reinvent the harness; every addition slots into `run.py` / `tier_b.json` / `judge.py`.
2. **Only deterministic metrics become blocking CI gates.** Live runs need Aseprite + tokens
   and are non-deterministic, so they stay on-demand; the *gate* is the deterministic layer
   (silhouette-IoU, the existing golden visual-diff, case well-formedness). CI stays fast +
   reproducible while the objective validator is still a real gate (the donut-test antidote).
3. **Silhouette-IoU over rendered PNGs, pure stdlib** (reuses `pixelpng.py`) — no Aseprite in
   the metric, no new dependency, unit-testable; it consumes the film-strip / preview PNGs the
   perception tools (SPEC-005) already emit, so it composes with the existing draw→see→verify
   loop rather than adding a parallel one.
4. **Treat SwordsBench as methodology, not gospel** (§D caveat: N=2, single-run, self-scored).
   Port the verbatim prompts + the drift focus; the persona A/B (#5) exists precisely to *test*
   — not assume — its "an artistic prompt helps" claim.
5. **Evidence-archived, re-derivable results** (the existing RESULTS.md rule): every recorded
   live row links to `evals/runs/<date>/` artifacts (judge prompts, raw JSON scores, the scored
   PNGs / tool outputs) so a number is never a trust-me log.
6. **Golden fixtures are hand-authored, committed, deterministic.** The drift goldens under
   `evals/fixtures/` are small animations checked into the repo (a clean walk + a deliberately
   warped copy), regenerated by a committed `evals/fixtures/make_fixtures.py` so they're
   reproducible. They are a **snapshot contract**: a golden change is a reviewed, intentional
   commit (a "stale" golden is updated like any visual-regression baseline, not worked around).

## Acceptance criteria
- [ ] `tools/silhouette_iou.py` is pure-Python, stdlib-only (reuses `pixelpng.read_png`),
      **unit-tested** on hand-computed cases: two identical masks → IoU `1.0`; a known mask
      translated by 1px (so |∩| and |∪| are computable by hand) → the exact expected ratio; a
      disjoint mask → `0.0`; and a film-strip PNG slices into N frames reporting N−1 adjacent
      IoUs + the min.
- [ ] `evals/run.py` gains `silhouette_iou_stable` + `silhouette_iou_detects_drift` (golden
      fixtures under `evals/fixtures/`), registered in `CHECKS` and mapped in `cases.json`;
      `python evals/run.py` **exits non-zero** when the drifted golden trips the floor → CI
      (`quality.yml`) blocks on animation drift.
- [ ] `evals/tier_b.json` gains ≥2 SwordsBench cases (verbatim prompts) with valid weighted
      rubrics; the existing `tier_b_cases_wellformed` CI check passes them.
- [ ] A documented long-session-degradation protocol + a `judge.py` helper that computes the
      degradation slope from quality snapshots; **one recorded run** in `RESULTS.md` with
      archived evidence under `evals/runs/<date>/`.
- [ ] A persona-A/B protocol + `judge.py --emit-ab`; **≥3 recorded blind A/B runs** with the
      mean Δscore, and an explicit adopt/reject decision (adopt iff mean Δ ≥ +0.05, consistent
      sign) for the persona line.
- [ ] A cross-path benchmark recorded in `evals/BENCHMARK.md` (SwordsBench with/without each
      path), evidence archived.
- [ ] Checklist **9.4 → 3**; **9.3** strengthened (silhouette-diff golden); **no new
      dependency**; CI stays green + deterministic.

## Eval (how we grade it)
Meta — the harness grades itself. **Tier-A (CI, deterministic):** the `silhouette_iou` unit
tests + the gate firing **red on a drifted golden** animation and **green on a stable** one;
`tier_b_cases_wellformed` covering the new SwordsBench cases. **Tier-B (recorded):** the
SwordsBench / long-session-degradation / persona-A/B / cross-path runs, each a row in
`RESULTS.md` / `BENCHMARK.md` re-derivable from `evals/runs/<date>/`.

## Traceability
- Module(s): `tools/silhouette_iou.py` (pure IoU metric), `evals/run.py` (Tier-A checks +
  `CHECKS` registry), `evals/cases.json` (check→component map), `evals/judge.py` (`--emit-ab`
  + degradation-slope helper), `evals/tier_b.json` (SwordsBench cases), `evals/fixtures/`
  (golden animations), `evals/RESULTS.md` + new `evals/BENCHMARK.md`,
  `.github/workflows/quality.yml` (the gate). Reuses `tools/pixelpng.py`,
  `tools/lint_sprite.py`, and the SPEC-005 `live_save_filmstrip` / `live_frame_diff` outputs.
- Test(s): `silhouette_iou` unit tests (identity / drift / strip-slice); the two new `run.py`
  checks; `tier_b_cases_wellformed` extended to the SwordsBench cases.
