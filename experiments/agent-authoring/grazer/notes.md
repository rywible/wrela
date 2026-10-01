# The grazer: authoring notes

Render budget: 15. Every render is a contact sheet saved as `iter-NN.png`. I also cropped and enlarged
tiles of existing PNGs with `sips` to look closer; that is not a render, it re-reads pixels already made.

## Plan before the first render

Laid out the skeleton numerically before writing any shape: picked joint positions so the bone
lengths match the design doc (fore 45/42/20 cm, hind 50/45/22 cm), with the stifle bending forward
and the hock back (hind), the humerus sloping down and back to an elbow that sits at the floor of the
chest, and a vertical forearm and cannon (fore). The hoof (about 10 cm with the pastern) makes up
the remaining height. Every part is built in its bone's frame (`bone_space`, `muscle`) or as a
round cone between two joints, and the whole field is evaluated on `mirror_x(p)` so the paired legs
are authored once.

## Iterations

### iter-01
- Saw: it reads as a quadruped at once, but as a horse. The legs are spindly. A ball-shaped brisket
  bulges under the neck like a goitre. The dapples are big camouflage blotches, and the hide is an
  orange tan, not a warm brown. From the front it's a narrow llama with round mouse ears. The hind
  leg has a separate "sausage" (the hamstring cone) hanging behind the thigh down to a knobbly hock.
  Diagnostics: gradient max 3.13, and the ear tips are reported as separate pieces.
- Changed: wider barrel, chest and rump; removed the brisket ball; thicker legs throughout; the
  thigh is now one broad mass offset backward in the femur frame, with the hamstrings blended into
  it; jittered-cell round dapples in place of thresholded noise; a darker, less saturated hide; the
  mouth slit is now a box (I suspected its eccentric ellipsoid bound of causing the gradient spike).

### iter-02
- Saw: bulkier, and the hind leg now reads correctly (thigh mass, gaskin, hock pointing back). Still
  a fat horse. The back line is one smooth concave curve with no hump. A "hip point" bump sticks up
  on the back like a tumour. The shoulder is an undefined blob, and the fore leg comes straight out of
  the belly as a tube. The ears face forward, so they look like Mickey Mouse ears from the front and
  a spike from the side. The gradient max got worse (3.64), so the mouth slit wasn't the cause.
- Diagnosis of the gradient: the library `sd_ellipsoid` bound is discontinuous at the ellipsoid's
  centre (its value there depends on direction, between -r_min and -r_max). Small ellipsoids
  (nostril, knee, hock, brow) have their centres within 10 cm of the surface, and the nostril is
  subtracted, so the discontinuity sits in the air inside the nostril.
- Changed: `ell()` uses the library bound outside and a Lipschitz-1 bound inside. Lowered the neck
  base so a shoulder hump (peak about 1.71 m) stands above the neck root, giving a bison top line
  that falls to the rump. The ears are now leaf-shaped (a squashed round cone, cupped) and face
  forward and outward. Added a dark woolly cape over the hump, neck, shoulders and upper fore legs
  (fbm displacement, with the field divided where it's applied to stay 1-Lipschitz), a throat
  beard, and a bigger forearm muscle. The hip point is smaller and lower.

### iter-03
- Saw: the gradient is fixed (max 1.28, 0.01% of samples; 1 piece). The woolly cape works: it looks
  like soft, lumpy fur, and the dark forequarters with lighter, dappled hindquarters read as bison.
  But the cape ends on a hard vertical line, like a jumper. The hump still doesn't read, because the
  neck's top line runs straight on from it. From the front it's a llama: long, thin neck column,
  narrow head, upright ears.
- Changed: a slanted, noisy cape boundary that follows the ribs (further back at the top). A round,
  thicker neck. Ears now stick out sideways like cattle ears. Broader cranium and muzzle. Wider
  chest, narrower rump.

### iter-04
- Saw: the neck is now a woolly column as wide as the body. It reads as a sheep or alpaca. The
  sideways ears help. The cape boundary is good.
- Changed: laterally squashed neck again (1.3). Rebuilt the head for a longer face: separate upper
  lip, chin, mouth line and angled nostril slits. Larger, more upright ears. Went to a head
  close-up next.

### iter-05 (head close-up, centre 0,1.85,1.75, radius 0.45)
- Saw: a plausible camel or horse head. Problems: the eye sits under a heavy brow, so it looks
  closed. The mouth slit plus a protruding chin make a "duck bill" gaping mouth from the side.
  From the front the muzzle is a round ball and the ears are big donkey ears.
- Changed: bigger eye set lower, with a lighter brow and a dark eye surround in the albedo. A
  thinner, shorter mouth slit and a smaller chin. A narrower muzzle and bigger nostrils. Lowered
  the neck into an S (forward at the base, upright near the head), so the shoulder hump finally
  stands above the neck root. Moved the mane ridge further up the neck so it doesn't fill the saddle.

### iter-06
- Saw: the hump reads now, a distinct peak behind the neck. The silhouette is a woolly
  camel-bison. The legs look short and spindly under all that mass, and the hooves are bulbs.
- Went to a leg close-up next.

### iter-07 (legs close-up, centre 0,0.45,0.45, radius 0.7)
- Saw: the hooves read as cloven (the cleft shows from the front) and the hind leg is correct
  (gaskin, hock pointing back, point of hock). The fore leg is a uniform tube with a fetlock knob.
  The front view shows the tail tuft dangling between the hind legs, which is correct. Gradient
  max 1.34 at this finer grid; traced to steep ramps in the cape mask (a 10 cm ramp times about
  3 cm of displacement).
- Changed: forearm tapers more strongly (0.10 to 0.048) with a bigger muscle at the top; slimmer
  cannons, knee and fetlock; slightly smaller hooves and pasterns; a stifle (patella) bump; belly
  raised 5 cm so the legs show more length; gentler cape ramps and a slightly bigger divisor.

### iter-08
- Saw (enlarged crop): gradient max 1.09. The real proportion error shows now: the chest and neck
  base stick out about 40 cm in front of the fore legs, so the fore legs look set too far back. On
  a real ungulate the point of the shoulder is only about 20 cm ahead of the forearm. The hip-point
  bump looks like a tumour. From the side the ear is seen edge-on.
- Changed: fore leg joints moved 8 cm forward (bone lengths kept) and the chest pulled back. Softer
  hip point. Turned the ear's face outward. Added a pale, elk-like rump patch.

### iter-09
- Saw: better fore/aft balance in the side view. Still a llama from the front: the neck is a
  column and the shoulders slope away from it.
- Changed: wider stance (legs 2 cm further out), broader hump, an un-squashed flare at the neck
  base. Added short bovine horns (a choice of mine, not in the brief), because the head without
  them kept reading as a camelid or llama; the ears moved below and behind the horns.

### iter-10
- Saw: the horns came out brown and lumpy, with a one-sided floating piece. Cause: the cape
  displacement was applied after the ears and horns were joined, so they got wool on them. My best
  explanation for the one-sided piece is that the noise is sampled at p, not at mirror_x(p), which
  makes the wool asymmetric.
- Changed: the ears and horns are joined after the wool. Longer horns.

### iter-11 (head close-up, centre 0,1.85,1.7, radius 0.5)
- Saw: the horns read as bovine horns from the front, dark and clean. The eye reads. The ears are
  needle-thin from above; their tips show as tiny separate pieces at a 1 cm grid.
- Changed: bigger ears.

### iter-12
- Saw (crop of iter-10's 3/4 view, then iter-12): the widened hump had become a ball, like a
  hunchback, with a large specular sheen. From the side a horn is a straight vertical spike, which
  reads as a goat or a unicorn.
- Changed: the hump is a narrower, taller ridge. Clumpier wool (an extra low-frequency term) and
  a warmer wool colour. Thicker ears.

### iter-13
- Changed: horns in three segments (out, up, then hooking forward), so they read as curved from
  the side too.
- Saw: horns read from all four views. The two leftover "pieces" are at the horn tips. I believe
  they're thin tips (r 7-9 mm) falling between 3 cm grid samples rather than real gaps, because the
  horn is a chain of overlapping round cones, but I didn't spend a render to confirm it.

### iter-14 (final)
- Changed: the hamstrings end higher and further forward, so the back of the hind leg curves in
  above the point of hock. Lighter wool on the forearm so it keeps its taper.
- Saw: small improvements to the hind leg's rear outline and the forearm. Stopped here with one
  render unused.

## Final diagnostics (iter-14 / final.jpg, centre 0,1.05,0.8, radius 1.5)

```
volume: 0.902 m^3 (about 947 kg at 1050 kg/m^3)
bounds: x [-0.36, 0.39]  y [0.00, 2.16]  z [-0.43, 2.03] m
pieces: 3 separate pieces (main body 0.9022 m^3; two ~0 m^3 at (+-0.20, 2.16, 1.82) = horn tips)
ground: nothing below y = 0; lowest inside sample at y = 0.003 m
framing: fits inside the box
gradient near the surface (|d| < 10 cm): max 1.04, 0.00% of samples above 1.2
```
The x bounds are asymmetric (-0.36 / 0.39) because the wool noise is sampled in world space, not
mirrored.

## Skeleton (metres; right side shown, left side is the mirror in x)

| joint | parent | position (x, y, z) | bone length |
|---|---|---|---|
| PELVIS | (root) | 0, 1.30, 0.00 | |
| SPINE | PELVIS | 0, 1.36, 0.50 | 0.50 |
| CHEST | SPINE | 0, 1.36, 1.00 | 0.50 |
| NECK0 | CHEST | 0, 1.28, 1.12 | 0.14 |
| NECK1 | NECK0 | 0, 1.48, 1.38 | 0.33 |
| NECK2 | NECK1 | 0, 1.68, 1.51 | 0.24 |
| NECK3 | NECK2 | 0, 1.86, 1.60 | 0.20 |
| HEAD (poll; pitched 0.65 rad down) | NECK3 | 0, 2.00, 1.66 | 0.15 |
| TAIL0 | PELVIS | 0, 1.36, -0.30 | 0.31 |
| TAIL1 | TAIL0 | 0, 1.30, -0.37 | 0.09 |
| TAIL2 | TAIL1 | 0, 1.20, -0.41 | 0.11 |
| TAIL3 | TAIL2 | 0, 1.07, -0.42 | 0.13 |
| TAIL4 | TAIL3 | 0, 0.94, -0.42 | 0.13 |
| TAIL5 | TAIL4 | 0, 0.81, -0.415 | 0.13 |
| TAIL_END (tip) | TAIL5 | 0, 0.68, -0.41 | 0.13 |
| HIP | PELVIS | 0.21, 1.18, 0.00 | 0.24 |
| STIFLE | HIP | 0.22, 0.71, 0.17 | 0.50 (femur) |
| HOCK | STIFLE | 0.20, 0.32, -0.055 | 0.45 (tibia) |
| HFETLOCK | HOCK | 0.20, 0.10, -0.035 | 0.22 (cannon) |
| SCAPULA | CHEST | 0.22, 1.48, 0.86 | 0.29 |
| SHOULDER | SCAPULA | 0.26, 1.12, 1.18 | 0.48 |
| ELBOW | SHOULDER | 0.23, 0.73, 0.96 | 0.45 (humerus) |
| KNEE (carpus) | ELBOW | 0.21, 0.31, 0.97 | 0.42 (forearm) |
| FFETLOCK | KNEE | 0.205, 0.10, 0.99 | 0.21 (cannon) |

Each fetlock carries a pastern and hoof below it, down to the ground (about 0.10 m). Attachments
in head space (x lateral, y up, z along the head): EYE, EAR_BASE, and HORN0 to HORN3 (rigid on
HEAD). NECK0 sits 8 cm below CHEST: it's the root of the neck at the front of the chest, lower than
the withers, so the chain rises from there.

How parts attach: leg parts are round cones between consecutive joints, plus `muscle()`
ellipsoids placed in a bone's own frame (`bone_space`: z along the bone, y anterior). The head is
built entirely in head space. The neck and tail are cones between their joints. Not yet
skinnable as written: the torso ellipsoids (barrel, chest, rump, hump, hip point), the hamstring
cone, the beard, dewlap and mane ridge, and all the colour and wool masks use absolute world
coordinates. They'd need re-expressing relative to PELVIS, SPINE, CHEST or NECK0 before they
could follow an animation.

## Self-assessment

**Does it read as a real animal from all four views?** Mostly yes, as some animal, though not
always the one intended. Side and three-quarter views read as a large woolly horned grazer: a
shoulder hump, a dark cape over the forequarters, a dappled, lighter hindquarter, correct joint
directions (stifle forward, hock back, elbow at the floor of the chest, vertical forearm), cloven
hooves, and a tail with a tuft. The front view is the weakest. A tall neck carrying the head
2 m up, above a pear-shaped body, still says "llama" or "moose calf" more than "bison", and the
horns are what rescue it. The top-down view is a plausible oval body with horns and ears.

**Weakest parts:**
1. The forms are soft and generic. There are few bony landmarks (point of shoulder, ribs,
   tuber coxae, pin bones), so the body looks inflated rather than built over a skeleton.
2. The fore legs. The design doc's short cannons, plus the thick wool, make them look stumpy.
3. The hindquarter is one smooth bean shape, without muscle separation.
4. The head is acceptable but cartoonish: the ball muzzle from the front, a sleepy eye.
5. Not truly skinnable yet (see above).

**What was hard about authoring through this loop:**
- The feedback is coarse. At the default framing a hoof is about 10 px and the eye about 3 px.
  Close-ups cost renders from a budget of 15, so I enlarged crops of existing sheets with `sips`.
  Most of my critiques came from those crops.
- Cause and effect are far apart. A blend radius or ellipsoid centre is a number, but what I see
  is a silhouette. Several changes did nothing visible (the hip point, the crest), and a few did
  something unexpected (the widened hump became a ball). I worked these out on paper (converting
  head-space offsets to world space by hand, checking where an ellipsoid's surface sits at a given
  z) before spending a render.
- Interactions are non-local. The wool displacement silently coated the horns and ears because
  of evaluation order. The library ellipsoid bound's discontinuity at its centre showed up as a
  "gradient 3.6" number with no location attached.
- Anatomy has to be carried in my head: which way is anterior in each bone frame, and what the
  head pitch does to "up".

**What would have helped most:**
1. Diagnostics that point at parts: "gradient > 1.2 at (x, y, z), dominated by part `nostril`".
   That needs the field to be a tree of named parts rather than one opaque function. A language
   feature with named, inspectable sub-fields would give it for free.
2. Per-part highlighting or isolation in the render (render just `fore_leg`, or tint it), and a
   cheap single-view render at higher resolution with an orthographic side view and a ground grid
   in metres, for measuring proportions against the design doc.
3. A real skeleton/attachment construct: declare joints and parent frames once, write each part
   in its joint's local frame, and let the system compose the transforms. That's what would make
   this skinnable, and would stop me hand-converting head-space vectors. It's general (frames and
   hierarchical transforms), not engine-specific.
4. Masks that travel with parts. Colour and displacement regions were written as world-space
   boxes and slabs, which is fragile. Per-part material and displacement, attached to the same
   part tree, would remove the ordering bugs.
