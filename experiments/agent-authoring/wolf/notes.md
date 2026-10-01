# Grey wolf as a signed distance field: authoring notes

Tool: `fieldview` (2x2 contact sheet: side from +x, front from +z, three-quarter, top-down).
Framing used for full-body sheets: `--center 0,0.5,0.07 --radius 0.95`.

## Iteration log

### iter-01 (first blockout)
Built from a joint list first (spine, neck, head, tail chain, left fore/hind limb mirrored).
Torso = ribcage + prosternum + withers + loin + croup ellipsoids; neck round cone + ruff;
head in its own pitched frame (cranium, cheeks, x-squashed muzzle and jaw, nose pad, brow,
eye spheres, extruded-triangle ears); limbs as round cones between joints; paws = pad + 4 toes.

Saw:
- Reads as a canid at once, but more coyote/jackal than wolf: neck long and nearly horizontal,
  head small and low, muzzle thin.
- Thigh is a detached "ham" bulge with a notch in front of it; hind leg reads lumpy.
- Paws are round slipper blobs, too big and too tall.
- Colour nearly uniform grey: saddle invisible, belly cream barely shows, no warm tones.
- Top-down: body is an even-width tube, no waist; head is a narrow spike from above.
- Ears thin spikes from the side, leaning back too far.
- Diagnostics: gradient max 3.13 (0.45% of samples > 1.2). Suspect: `sd_ellipsoid`'s bound is
  discontinuous at each ellipsoid's centre (it tends to -r_axis along each axis), which lands inside
  the 10 cm band for small/thin ellipsoids; plus tail fbm.

### iter-02
Changed: raised the head and set the neck at ~45 degrees (head bone at (0, 0.93, 0.60), pitched
0.25 rad nose-down), head scaled 1.08, cheek ruff added, thigh rebuilt as sideways-flattened
round cones (new helper `sd_flat_cone`: scale x, divide by the scale, still a bound), smaller
flatter paws, more colour contrast. Replaced `sd_ellipsoid` with `sd_ell`: library bound outside,
`(k0 - 1) * r_min` inside (a true lower bound, continuous at the centre).

Saw:
- Gradient fixed: max 1.13, 0% of samples above 1.2. The centre discontinuity was the cause.
- Raised head and thick neck help a lot: now a canid with a proper neck carriage.
- Still the worst thing: the stifle is a round knob with a deep notch above it, and a thin gaskin
  below, so the hind leg reads as "ball on a stick". A canine hind leg in profile is a broad wedge
  from croup to hock (about 25 cm front-to-back at stifle height).
- Front view: a small nub hangs below the chest between the forelegs (prosternum ellipsoid sits too
  low; in a canine the lowest point of the chest is behind the elbows, not at the front).
- Ears tall, narrow and sharp: German-shepherd/coyote, not wolf. Wolf ears are shorter, broader,
  rounder-tipped, set wider. Muzzle still a bit thin for a wolf.
- Top-down: still an even tube; ribcage not wider than the waist.
- Colour: saddle shows a little in the three-quarter view, but overall still one beige-grey.

### iter-03
Changed: hind leg rebuilt as a wedge (upper thigh, hamstrings, deep gaskin cone, Achilles, plus a
flank-fold web from belly to stifle); ears shorter/broader/wider-set and turned outward; fuller
muzzle and jaw; ribcage wider, loin narrower; more colour contrast and a lower saddle edge.

Saw:
- Hind leg now reads as a canine hind leg: broad thigh, tapering gaskin, visible hock. Big win.
- Saddle visible from side and three-quarter; the colour pattern starts to say "wolf".
- Measured off the side tile (231 px/m): chest-to-buttock is ~1.25x shoulder height, which is what
  the brief's numbers imply, but the topline sags behind the withers and the croup is as high as
  the withers, so the body reads long and flat. Wolves are highest at the withers.
- Front view: the "nub" is the bottom of the ribcage showing between the forelegs below elbow level
  (dark in shadow). Chest bottom should sit at the elbows.
- Ears: from the side they are thin dark flags (flat slab seen edge-on); from above the dark rims
  read as a black band across the head.
- Front half lacks the wolf's heavy mane; hindquarters look bulkier than the forequarters.
- Outer thigh is cream (ventral colour line too high at the rear).

### iter-04
Changed: lowered loin/croup/tail root ~1.2 cm so the withers are the high point; raised the chest
bottom to 0.435 m; bigger neck cone plus a ruff and a mane ellipsoid over the withers (fbm-roughened);
ears rebuilt as a cupped wedge (thick at the back of the base, recessed inset triangle for the
opening); tail thinner at the root, fuller in the middle; outer thigh no longer cream.

Saw:
- Ears now read as triangles from the side, cream-lined from the front. Good.
- Topline slopes gently from withers to croup; mane makes the forequarters heavier. More wolf.
- Front-view nub still there, so it was never the ribcage: it is the two inner thighs meeting at the
  midline below the belly (thigh cones reach x ~ 0.0), seen under the chest between the forelegs.
- Forearms a bit spindly; tail is a smooth sausage with a blunt black end.
- Head looks small and doggy next to the bigger mane; needs a close-up.

### iter-05 (full body)
Changed: inner-thigh cut (`smax(d, 0.022 - m.x, 0.02)`) so the hind legs never meet at the midline;
sturdier forearm; fluffier tail (fbm amplitude up, whole tail field scaled by 0.88 to keep the
gradient bounded); dark stripe down the front of each forearm.

Saw: almost no visible change at full-body scale, and the front-view "nub" was still there. So I
was wrong twice: the nub is the black tail tip hanging between the hind legs, seen under the belly
through the gap between the forelegs. It is correct, not a modelling error. Lesson: a dark blob in
one view needs to be cross-checked against the other views before "fixing" it.

### iter-06 (head close-up, `--center 0,0.9,0.62 --radius 0.24`)
Saw:
- Ears are broken: the recess cut straight through, leaving an open triangular frame, and the thick
  base is a flat shelf that sticks out backwards (two bars in the top view).
- Muzzle is a long tapered cylinder with a round ball nose: fox/borzoi, not wolf. A wolf muzzle is
  boxy (flat top, flat-ish sides) and deep.
- The mouth is a wide dark slot (the gap between muzzle cone and jaw cone, painted dark too).
- Eyes are round beads with no eyeliner; wolves have almond, oblique eyes with dark rims.
- Neck end of the neck cone (r 0.088) is wider than the skull, and the throat ruff reads as a pouch,
  so the head looks small and the jaw line is lost.

### iter-07 (head close-up)
Changed: ears rebuilt as a round cone squashed front-to-back (z scaled 1.8) with a smaller offset
cone subtracted from the front (`sd_ear_cavity`, also used by the albedo for the cream inner fur);
muzzle is now a tapered rounded box (`sd_muzzle`); jaw blended with a tight k; neck cone end thinned
to r 0.075; almond dark eyeliner tilted with the outer corner higher; lip line follows the muzzle
bottom and curls up at the corner.

Saw:
- Ears now read as real cupped ears from every view. Biggest single improvement to the face.
- Eyes read as wolf eyes (amber, dark-rimmed), but the liner is too big: cartoon-angry.
- Boxy muzzle profile is better, but from above the muzzle is a narrow spike against wide cheeks.
- Lower jaw sticks out as a separate thin bar ("duck bill"); its tip shows as a pale tongue under
  the nose in the front view.
- Lip line runs back across the cheek fur, so from the front it is a long horizontal slash.
- Nose is a big black ball from the front.
- Forehead too pale; the face pattern is washed out.

### iter-08 (full body)
Changed: broader muzzle base (hx 0.043 -> 0.022), jaw shorter and better blended, smaller nose pad,
lip line limited to the muzzle, smaller eyeliner, darker crown and a tawny-brown muzzle stripe,
rounder ear tips.

Saw:
- Front and three-quarter views now have a believable wolf face at full-body scale.
- Measured from the side tile: chest bottom at 54% of shoulder height, i.e. leg length is right;
  the "too long" impression comes from a long, flat, boxy croup (buttocks as high as the loin) and a
  long thin loin. A wolf's croup slopes down to the tail root.
- Tail ends in a blunt black club; should taper.
- Top-down: still little waist.
- Forearms are uniform sticks (no muscle at the top). Ears look small at this scale.
- Whole surface is very smooth: reads as plastic rather than fur.

### iter-09 (full body)
Changed: croup ellipsoid tilted 0.30 rad so the rump slopes to a lower tail root; ribcage 1.5 cm
longer, loin narrower; tail tapers to a point; forearm thicker at the top; slightly larger ears and
head; whole-body fur breakup (fbm stretched along z, 4.5 mm, faded out on the lower legs), with the
whole field scaled by 0.9 to keep the gradient bounded (max 1.09).

Saw:
- Fur breakup turns the plastic look into something furry. Croup now slopes; tail tapers. The side
  view reads as a wolf.
- New problem: diagnostics report a second, tiny piece at (-0.09, 1.00, 0.57): the fur noise
  pinched off a sliver of the thin ear rim on one side only.
- Head still looks a little small against the neck and mane from the side and the front.
- Paws are pale blobs at this scale; need a close look at legs and paws.

### iter-10 (legs close-up, `--center 0,0.22,-0.05 --radius 0.5`)
Changed (before this render): fur noise faded to 20% on and near the head (fixes the ear sliver;
verified on the next full-body render), bigger cheek ruff, slightly thinner neck base.

Saw:
- Fur breakup looks good up close: shaggy, not plastic. Hind leg reads correctly (thigh, gaskin,
  hock angle, near-vertical metatarsus). Belly tuck and flank are right.
- Paws are too small for a wolf (about 9.6 x 8 cm; wolf forepaws are ~11-12 x 9-10 cm) and the
  toes are round pebbles with no claws.
- Top of the foreleg has a round knob in the three-quarter view: the humerus/triceps cone ends make
  the elbow stick out. A wolf's elbows sit tight against the chest.

### iter-11 (full body)
Changed: paws scaled up (fore 1.25, hind 1.12) with tighter toe blends and dark claw tips painted
from the same toe constants (`claw_mask`); humerus and triceps cones end higher and further in so
the elbow sits against the chest.

Saw:
- One piece again (ear sliver fixed). Paws now read as big wolf paws.
- From the side the head still looks small and the neck long; ears are seen almost edge-on and
  read as nubs (cup too shallow front-to-back).
- The waist tuck is shallow, so the trunk reads as a tube.
- Front view: chest front is grey; a wolf's throat and chest front are pale.

### iter-12 (full body)
Changed: head bone lowered/back 2 cm and pitched 0.22 (shorter-looking neck); ear cup deeper
front-to-back (z squash 1.8 -> 1.45) and 4 mm taller; loin raised and slimmer for more tuck; pale
throat/chest bib widened.

Saw:
- Ears now read as triangles from the side as well as the front. Neck reads shorter.
- Chest front still looks grey from the front, but the side tile shows it cream: it is in shade
  under this light, not an albedo bug. Left it.
- Remaining "not quite wolf" cues are silhouette ones: the outline is still smooth everywhere, while
  a wolf's outline is broken by a belly fringe, fluffy "britches" behind the thighs, and raised
  hackles on the neck. The muzzle tip reads a touch pointed.

### iter-13 (full body)
Changed: blunter muzzle tip (taller at the nose); hackles ellipsoid along the top of the neck;
hanging-fur displacement (anisotropic noise, 1 cm) on the belly underline and on the back of the
thighs ("britches"), inside the same 0.9-scaled field (gradient max 1.15).

Saw: the neck topline is now slightly convex (hackles), which is more wolf. Belly fringe and
britches are barely visible at full-body scale: correct in spirit, too subtle to matter at 512 px.
No regressions; one piece; nothing below ground.

### iter-14 (head close-up, `--center 0,0.9,0.63 --radius 0.25`)
Saw:
- Regression: the amber irises are gone. Widening the muzzle base in iter-08 swallowed the eye
  spheres (eye centre x = 0.036, face surface now at x ~ 0.043), so only the painted almond liner
  shows. I never re-checked the eyes after changing the muzzle; full-body renders were too small to
  show it.
- The cream inner-ear colour leaks to the outside of the ear base as a pale ring (the cavity SDF is
  within 6 mm of the thin outer shell there).
- Muzzle reads a bit too boxy from the side (hard top edge, squared front).
- Good: broad cheek ruff from the front, cupped ears, pale throat, nose size, darker crown.

### iter-15 (full body, final)
Changed: eye centres moved out to x = 0.040 (r 0.010) so they sit proud of the wider muzzle; iris
and pupil painted around a forward-outward gaze direction instead of the head's x axis; inner-ear
cream limited to the front of the ear (`e.z > 0`); muzzle corner radius increased (less boxy).

Saw: amber eyes visible again in the side and three-quarter tiles; no ring on the ear backs; no
regressions; one piece; nothing below ground; gradient max 1.15. This was the 15th render, so the
eye fix was **not** checked in a close-up: at full-body scale the eyes are only a few pixels.

After the last render I renamed the inline ear-base offset to the named constant `EAR_BASE` (pure
rename, same value). It was checked with `naga` (lib.wgsl + creature.wgsl validate), not rendered.

`final.jpg` = `iter-15.jpg`. A copy of the iter-13 field was kept as a fallback in the session
scratchpad (not in this folder).

## Skeleton

Units: metres; +y up, +z forward, +x = the wolf's left. Limb and ear/eye joints are given for the
left side; the right side is `mirror_x`. Head-frame joints are in the head bone's local frame
(world = J_HEAD + rot_x(local * 1.13, 0.22)); approximate world positions are given too.

| Joint | Parent | Position |
|---|---|---|
| J_PELVIS (root) | none | (0, 0.632, -0.430) |
| J_LUMBAR | J_PELVIS | (0, 0.655, -0.180) |
| J_CHEST | J_LUMBAR | (0, 0.615, 0.095) |
| J_WITHERS | J_CHEST | (0, 0.800, 0.200) |
| J_NECK | J_CHEST | (0, 0.680, 0.310) |
| J_HEAD (pitch 0.22 rad, scale 1.13) | J_NECK | (0, 0.912, 0.592) |
| EAR_BASE L/R | J_HEAD | head frame (±0.046, 0.040, -0.036), world ≈ (±0.052, 0.965, 0.562) |
| EYE_C L/R | J_HEAD | head frame (±0.040, 0.018, 0.052), world ≈ (±0.045, 0.919, 0.654) |
| J_TAIL0 | J_PELVIS | (0, 0.668, -0.570) |
| J_TAIL1 | J_TAIL0 | (0, 0.585, -0.660) |
| J_TAIL2 | J_TAIL1 | (0, 0.450, -0.715) |
| J_TAIL3 (tip) | J_TAIL2 | (0, 0.270, -0.690) |
| J_SCAPULA L/R | J_WITHERS | (±0.050, 0.750, 0.185) |
| J_SHOULDER L/R | J_SCAPULA | (±0.065, 0.565, 0.335) |
| J_ELBOW L/R | J_SHOULDER | (±0.078, 0.415, 0.245) |
| J_CARPUS L/R | J_ELBOW | (±0.072, 0.128, 0.270) |
| J_FPAW L/R | J_CARPUS | (±0.070, 0.000, 0.300) |
| J_HIP L/R | J_PELVIS | (±0.070, 0.600, -0.420) |
| J_STIFLE L/R | J_HIP | (±0.085, 0.410, -0.320) |
| J_HOCK L/R | J_STIFLE | (±0.078, 0.210, -0.495) |
| J_HPAW L/R | J_HOCK | (±0.075, 0.000, -0.445) |

Parts and the bone each hangs on: torso ellipsoids (ribcage on J_CHEST, withers on J_WITHERS, loin
on J_LUMBAR, croup on J_PELVIS, tilted); neck cone J_NECK -> head, plus ruff, mane and hackles on
J_NECK; head parts (cranium, cheeks, cheek ruff, muzzle box, jaw, nose, brow, eyes, ears) in the
head frame; limb segments as round cones between consecutive joints; paws in a frame at
J_FPAW/J_HPAW (toe constants TOE_IN/TOE_OUT); tail as three round cones along the tail chain.
The jaw has no joint of its own (mouth closed); a rig would need one.

Derived numbers (from the constants, not measured off the renders): nose tip ≈ (0, 0.870, 0.813),
so nose to tail root ≈ 1.38 m; withers 0.80 m; tail chain length ≈ 0.45 m; chest bottom ≈ 0.435 m.

## Self-assessment

**Does it read as a real wolf from all four views?**
- **Side and three-quarter:** yes, as a grey wolf. It has a deep chest, a tucked waist, long legs,
  a visible hock, big paws, a thick neck with mane and hackles, a sloping croup, a hanging bushy
  tail with a dark tip, a dark saddle over grey sides, tawny legs, and a pale belly and throat.
- **Front:** mostly yes. Cupped ears with pale insides, broad cheek ruff, pale throat, narrow chest,
  legs close together. The head looks a bit small for the body from the front.
- **Top-down:** the weakest view. The body is a near-uniform tube. A wolf from above has a visible
  ribcage-to-waist-to-hips rhythm, and that only shows faintly here.

**Weakest points:**
- The outline is still smooth. The fur is only millimetre noise, plus a belly fringe and britches
  that don't show at this resolution. There are no real guard-hair silhouettes.
- The muzzle is a little boxy and dog-like in profile.
- Mass. The tool reports 0.095 m^3, about 100 kg at flesh density. A real wolf of this size is about
  40-55 kg. Fur that is modelled as solid volume explains part of the gap, but probably not all of
  it. My guess is the neck, mane and thighs are too bulky. I didn't test this.
- The final eye fix was never seen in a close-up.

**What was hard about this loop:**
- **Feedback per render is coarse.** Each full-body tile is about 230 px per metre, so the face,
  eyes and paws are a handful of pixels. Two of the 15 renders went on close-ups just to see the head.
- **Changes have side effects I can't see.** Widening the muzzle buried the eyes (iter-08). I didn't
  notice until a close-up six renders later.
- **I misread a symptom.** A dark "nub" in the front view cost two changes to fix the wrong thing
  (ribcage, then inner thighs). It was the tail tip, visible through the legs.
- **No numeric probe.** I couldn't ask "what is the surface x at the eye?" or "how wide is the waist
  from above?". I did those calculations by hand in my head from the formulas, and that is how the
  eye burial slipped through.
- **I had to keep the gradient bound by hand.** I divided squashed primitives by their scale and
  multiplied noisy fields by 0.88 and 0.9. The library ellipsoid's discontinuity at its centre was
  the cause of the gradient spike in iter-01 (max 3.13).

**What would have helped most:**
1. **A probe/measure tool.** Point queries (distance, nearest part), and orthographic silhouettes or
   cross-sections with a metric grid. Most of my reasoning was really measurement done in my head.
2. **A part-ID false-colour view.** Colour each surface point by the part that wins the union. That
   would have explained the nub and the buried eyes at once.
3. **Language level (general, not engine-specific):**
   - Values that carry their own Lipschitz bound through scaling and composition, so an
     "is this still a valid distance bound?" check comes from the compiler, not hand-tuned factors.
   - Named local frames as first-class values, so parts declare the bone they hang on, rather than
     re-deriving `rot_x(p - J_HEAD, -pitch) / scale` in both the field and the albedo.
   - Cheap assertions over functions that run on every build, for example "the eye surface stands
     at least 3 mm proud of the face". These would have caught the regression without spending a
     render.
