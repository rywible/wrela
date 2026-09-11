# Workshop delivery · September 11, 2026

The soundstage now separates a field subject from its lighting rig. It authors
shared game tree recipes and arbitrary bounded field expressions, with native
shape/material controls, camera fitting, close views, scale reference, grove
arrangements, exact studies and a comparison/motion viewer. See
[SOUNDSTAGE.md](SOUNDSTAGE.md) for the authoring loop and source schema.

## Exercised authoring loop

- Edited the tree in the native panel and through the protocol; typical uncached
  field rebuilds took 260–314 ms. Reusing cached recipes took under 1 ms.
- Saved `Games/Sanctuary/Authoring/Assets/tree.json` and verified the garden loaded its parameters.
  Growth recipe: 5.8 m height, 6.8 m crown width, 0.27 m trunk radius, 1.2 branch
  spread, 2,200 leaves. Actual bounds include the root/canopy surfaces.
- Inspected seed, custom stone vessel, tree and calibration spheres. Used native
  light selection, scale-reference toggle, shape slider and camera fitting.
- Inspected source comparisons in the in-app browser, moved its comparison
  divider, and played/paused a captured orbit sequence.
- Quit/reopened the native workshop and verified session restoration.
- Walked the garden and checked the shared tree and renderer in context.

Using the tool exposed and fixed camera fitting for wide calibration objects and
small subjects with a human scale reference, camera/light coupling on close-up
views, negative-position labels, and floor edges. Open object rigs now use an
analytic infinite plane with the shared PBR shader and shadow map. Outdoor ground
uses distance attenuation toward the same atmospheric horizon lookup. The room
keeps its physical enclosure. The plane has no mesh extent or marching loop.

## Reproducible visual evidence

Local reports are under `.soundstage/studies/`:

- `workshop-finished-20260911-075006/index.html`: final tree under noon, golden hour,
  sunset, afterglow, overcast, rain, softbox, overhead, lamp and room, each from
  quarter and back views. Includes 20 native 1080p captures and their metadata.
  `contact.jpg` gives an overview. Open rigs have continuous ground to the horizon.
- `tree-authoring-20260911-072141/index.html`: earlier source comparison. These
  historical images still contain the finite floor that prompted the horizon fix.
- `tree-motion-20260911-072814/index.html`: eight matched frames per source,
  orbiting through a complete turn. Comparison times and physical camera distances
  match; motion can be scrubbed. This predates the final floor replacement.
- Final calibration framing and seed close-up captures are in
  `.soundstage/captures/` with labels `calibration-final-fit` and `seed-final-softbox`.

Captured sequences are deterministic reference playback, not a test of live sky
cache reprojection. Individual capture metadata records exact source/settings,
shader fingerprints, time and dimensions. The comparison viewer hides irrelevant
frame/play or comparison controls when a study has only one frame or variant.

## Checks and short performance measurements

- Swift tests: 20 passed, including wind state JSON round-trip and exact future
  evolution after restoration.
- Metal renderer compilation: passed on Apple M4, including the plane pipeline.
- Soundstage validation: 15 passed.
- Workshop validation: 17 passed, including rejected edit transactions, exact
  saved-study pixels, exact future wind/turntable pixels and independent lights.
- Garden validation: 9 passed.

Final tree soundstage, live sky and wind, native 1080p, 4× MSAA, 10.2 seconds:
59.93 submitted fps; GPU median 5.48 ms, p95 12.61 ms, max 14.57 ms. Frame interval
median 16.67 ms, p95 17.57 ms, max 20.67 ms. Cloud updates remain included.
Report: `.soundstage/profiles/workshop-final-ground-20260911-075050.json`.

Garden check earlier in this delivery, with the saved tree and about 4.51 million
visible triangles, over 20 seconds: 55.73 submitted fps; GPU median 11.73 ms,
p95 13.76 ms, p99 19.77 ms. Frame interval p95 30.44 ms; thermal state nominal.
This is not a clean sustained 60 fps result: presentation/update hitches remain.
Report: `.sanctuary/profiles/workshop-garden-20260911-073201.json`.
These are short development measurements, not a prolonged thermal benchmark.

## Current limits

Indoor sources use inverse-square point lighting with a finite-source highlight
approximation and filtered shadow penumbrae. The softbox is not an integrated
area emitter, and room fill is not full scene GI. Intensity is not calibrated lux.
Outdoor aerial perspective on the authoring plane is a homogeneous attenuation
approximation; it is not curved-Earth terrain or a full aerial-perspective volume.
Clouds and wind are procedural simulations rather than a thermodynamic weather
solver. Tree appearance still needs a dedicated authoring pass for convincing
branch structure, leaf grouping and material detail; this delivery builds and
exercises that workflow rather than certifying the tree as finished game art.
