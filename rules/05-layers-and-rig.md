# 05 — Layers & rig conventions

> Checklist 4.5. The standard layer/rig so sprites are animatable, reviewable,
> and consistent across the project. Set this up **before** drawing.

## 1. Why rig in layers
- Animating a layered rig (move/redraw a limb layer per frame) keeps volume and
  palette consistent and avoids redrawing the whole sprite each frame.
- Each layer must read as **a clean, meaningful shape on its own** (hide the
  others and check) — a half-cut limb on a layer is a smell. (Past failure: chin
  ended up on the Body layer; arms baked into the torso.)

## 2. Standard character rig (bottom → top)
Draw order matters; list bottom layer first.

| Order | Layer | Contains | Notes |
|------:|-------|----------|-------|
| 1 | `Shadow` | ground contact ellipse | Separate so it can stay put while body bobs |
| 2 | `Legs` | both legs + feet | Or split `LegL`/`LegR` for 3/4 depth stagger |
| 3 | `Body` | torso **+ clothing/shorts**, neck base, shoulders | Complete from neck down; clothing merged in |
| 4 | `ArmL` | far/left arm + hand | Behind the torso in 3/4 |
| 5 | `ArmR` | near/right arm + hand (+ held weapon) | Weapon lives with the holding hand |
| 6 | `Head` | head, face, ears, **chin**, nose, hair | Includes the chin; no neck/shoulder pixels |

Rules:
- **Chin belongs to Head; shoulders/neck-base belong to Body.** Body is drawn
  complete from the neck down (don't leave a hole the head must cover).
- **Arms are their own layers** (`ArmL`, `ArmR`), never baked into Body, so they
  can swing independently.
- The **weapon rides with the hand layer** that holds it (`ArmR`), so it follows
  the swing automatically.
- For 3/4 view, prefer splitting paired limbs (`LegL`/`LegR`, keep `ArmL`/`ArmR`)
  so near/far depth and occlusion are controllable.

## 3. Naming conventions
- `PascalCase` semantic names: `Head`, `ArmR`, `LegL`, `Shadow`, `Weapon` (if not
  merged into the arm).
- Suffix `L`/`R` from the **character's** point of view, stated once and kept.
- Group related layers under a group layer per part when complex (e.g. `Arms`
  group → `ArmL`, `ArmR`). Frames/tags name the action: `idle`, `walk`, `attack`.
- An AI-draft scratch layer (if used) is clearly named and removed before export.

## 4. Per-layer cleanliness (must pass)
For **each** layer, hide all others and verify:
- [ ] It is a complete, recognizable shape on its own (no orphaned half-limb).
- [ ] No stray pixels bleeding from a neighboring part (no "lem nhem" at seams).
- [ ] Its anatomy is correct (chin on Head, shoulders on Body, etc.).
- [ ] It uses the locked palette/ramps for its material.
- [ ] Its registration (position) lines up with the others when all are shown.

## 5. Pivot & registration
- Keep a consistent pivot (e.g., feet-center on the ground line) so frames and
  the shadow align. Don't let the silhouette drift frame-to-frame except as the
  motion intends.
- Keep the canvas large enough to hold the widest animation pose (wind-up,
  weapon overshoot) without clipping.

## Do / Don't
| Do | Don't |
|----|-------|
| One clean shape per layer | Bake arms/weapon into the torso |
| Chin on Head; shoulders on Body | Split anatomy across the wrong layers |
| Weapon on the holding-hand layer | Put the weapon on a disconnected layer |
| Semantic PascalCase + L/R names | `Layer 1`, `Layer 2`, ambiguous sides |
| Check each layer hidden-solo | Only ever judge the merged result |
