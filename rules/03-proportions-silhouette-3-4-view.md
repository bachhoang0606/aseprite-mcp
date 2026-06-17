# 03 — Proportions, silhouette, 3/4 view

> Checklist 4.3. Reference angles, sizes, and readability.

## 1. Silhouette readability (the gatekeeper)
- Test: fill the whole sprite with one flat color. Is it instantly identifiable?
  Is it distinct from other characters in the game at a glance?
- **Break the silhouette at the extremities** — give the head a distinct shape, a
  weapon a clear protruding form, hands and feet readable bumps. Internal detail
  cannot rescue a blobby outline.
- Avoid **tangents**: edges that just barely touch (weapon tip grazing the head
  outline, two limbs whose outlines merge) — they flatten depth. Add a gap or
  overlap clearly.

## 2. Proportions (stylized, not anatomical)
- Game characters are usually **chibi / heroic**, not realistic. Realistic 7.5-head
  proportions look wrong and waste pixels at small sizes.
- Rules of thumb by size:
  - **Tiny (16–24px):** ~2 heads tall. Big head, tiny body, no facial detail
    beyond eyes. Readability over realism.
  - **Small character (32–48px):** ~2.5–3.5 heads tall. Eyes + simple features;
    hands as mitts.
  - **Detailed (48–64px+):** ~4–5 heads tall. Room for face, fingers-as-hints,
    costume detail.
- Keep the **head and hands slightly oversized** for readability and appeal — that
  is the stylization, not an error.

## 3. Goblin proportions (project reference)
- Goblin = squat, hunched, long arms, big head, pointed ears, big nose, big hands.
  Lean into a low, wide, asymmetric silhouette. The weapon (club) is a big,
  readable shape — never a tiny stub (a past failure mode). See
  `knowledge/references/goblin.md`.

## 4. Views & their conventions
Decide the view up front; it changes how you shade and rig.

- **Side (profile):** simplest; one ear, nose breaks the outline, near limbs
  overlap far limbs. Good for platformers.
- **Front (orthographic):** symmetric; readable face; weak depth — use shading,
  not foreshortening, to show form. Watch for it looking "flat/standing on toes."
- **Top-down:** see the head/shoulders most; limbs read by their outline from
  above; little face.
- **3/4 (three-quarter):** the workhorse for RPGs/iso. See below.

## 5. 3/4 view (do it right)
3/4 = the camera is rotated ~45° horizontally **and** tilted down a bit, so you
see the **front + one side + a little of the top** of the subject.

- **Asymmetry is the point.** The face is not mirror-symmetric: the **far side is
  foreshortened** (compressed, fewer pixels) and the **near side is wider**. The
  nose sits off-center toward the far edge.
- **Show planar turn:** the body is a box turned 45° — front plane (base color),
  side plane (one ramp step darker), top plane of shoulders/head (one step
  lighter, since it catches top light). This planar shading is what *sells* 3/4.
- **Feet/stance:** the two feet are at slightly different depths (one nearer,
  larger/lower; one farther, smaller/higher) — not side-by-side on one line.
- **Far-side limbs** are partly occluded by the torso and read smaller; near-side
  limbs overlap in front.
- **Eyes/ears:** the far eye is slightly smaller and closer to the center line;
  the far ear may be hidden or just a sliver.
- Keep the down-tilt consistent: you should see a little of the **tops** of
  shoulders, head, and any held object.

### 3/4 quick recipe
1. Block the box silhouette turned 45° (wider near side).
2. Mark the three planes (front / side / top) and assign ramp steps.
3. Place features off-center toward the far edge; foreshorten the far half.
4. Stagger the feet in depth; overlap near limbs over the torso.
5. Shade by plane first, then add form shadow within each plane.

## Do / Don't
| Do | Don't |
|----|-------|
| Pass the flat-silhouette test first | Rely on interior detail to identify it |
| Stylize (big head/hands) at small sizes | Use realistic proportions on tiny sprites |
| Make 3/4 asymmetric + planar-shaded | Draw a "front view nudged sideways" |
| Stagger feet in depth for 3/4 | Plant both feet on one horizontal line |
| Make the goblin club a big readable shape | Shrink the weapon to a stub |
