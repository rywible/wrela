# Sanctuary native art iteration — September 12, 2026

Astra alone owns all 3D models and animation, per user instruction. This is an
iteration report, not an accepted baseline or full-game completion claim.

## Source and observed changes

`Games/Sanctuary/Project/LivingWorldPresentation.swift` supplies shared game/studio
geometry and pose functions. Moonhart now has a connected chest/neck/muzzle, eyes,
ears, branching antlers and tapered articulated legs. Cloud Ray uses a sampled
parametric wing with smooth skin deformation. Cabin eaves/gables cover its unchanged
collision footprint. Swift compilation initially timed out on a complex closure;
Astra split its Float arithmetic and both app builds then passed.

Focused quick: `.build/test-runs/20260912-163134-quick-30b1b/index.html`.
Actual 1080p captures are indexed by `.build/sanctuary-native-20260912/astra-captures.json`
and `astra-motion-captures.json`. Each PNG has exact metadata. The saved subjects
are `moonhart-astra.json`, `cloudRay-astra.json`, `construction-cabin-astra.json`
in that native evidence directory. Root reviewed quarter silhouettes, Moonhart
side contact/peak-hop/full-blink and Cloud Ray side flight. Astra reviewed all
nine static captures. Models remain intentionally simple; Moonhart lower legs and
shoulder joins still look toy-like. Ray tail has a visible radius step. Cabin roof
edges staircase and the bark-like interior/floor plus ochre window blocks need
refinement. No global exposure compensation was applied.

Native Frostling Behavior controls selected startle then Run 10 seconds. The shared
brain changed watching → fleeing at 1.12 s, fear 1.00, and the actual screenshot
showed it turned away. Earlier native Motion controls captured its full hop strip.
Moonhart/Cloud Ray use shared pose clips; they do not yet expose species-specific
studio behavior scenarios.

## Shared look

`stagectl styleBoard` generated
`astra-review2/workspace/.soundstage/studies/style-board-20260912-163451-31f8a4/`
under the native evidence directory. Root read its HTML and inspected candidate
noon/sunset, reference tree noon and noon sky PNGs. The candidate remains legible
in shared lighting; sunset strongly warms every material, while noon leaves its
body relatively dark blue-gray. Existing reference tree is much noisier than the
creature; reference status remains provisional. No style baseline was approved.
Interactive local-file browser viewing was rejected by automatic browser policy;
no bypass was attempted. This review used HTML source plus actual captured PNGs.
The complete front/quarter/back/above × all-weather suite remains outstanding.

## Replay

Build Soundstage with `./scripts/build Soundstage`, launch an isolated harness app,
set WRELA_CONTROL_ROOT to its `.soundstage` directory, then:

```sh
./scripts/stagectl project sanctuary
./scripts/stagectl catalog
./scripts/stagectl subject moonhart
./scripts/stagectl rig softbox
./scripts/stagectl view right
./scripts/stagectl pause true
./scripts/stagectl creature --mode hop --seconds 0.4
./scripts/stagectl creature --mode idle --seconds 3.85
./scripts/stagectl subject cloudRay
./scripts/stagectl creature --mode flight --seconds 2.18
```

Compiled source/shader provenance and artifact hashes are in
`.build/sanctuary-native-20260912/astra-review-manifest.json`.

## Second candidate: complete creature/cabin lighting review

Astra inspected all 32 actual PNGs for each of Cloud Ray, Moonhart and cabin:
front/quarter/back/above under noon, golden hour, sunset, afterglow, overcast,
rain, indoor and softbox. HTML source and report metadata were read from the
filesystem; no browser bypass or baseline approval occurred. Studies are beneath
`.build/sanctuary-native-20260912/astra-second-review/workspace/.soundstage/studies/`:

- `cloudRay-second-conditions-20260912-170256-011bf9`
- `moonhart-second-conditions-20260912-170327-257a0d`
- `cabin-second-conditions-20260912-170129-512ef4`

These captures use source digest
`f6d4b7bb939d2f7d17827968a186313f4f01bddd01d89cdb7ae0e5d0e24eb34b`
and shader digest
`c32257ce930b722659fe1ab5907751485051575d0b5022447662912975dfae1b`.
They precede the subsequent world-elevation integration. Exposure remains 1 with
the shared scene look. Every condition-study frame has wetness 0, including rain.

Cloud Ray has a continuous rounded rim and tapered tail, with clear studio floor
clearance. Its broad face and black eyes read as a simple friendly creature under
noon and studio lights. The head/wing intersection remains conspicuous in quarter
and above views, and the protruding bead eyes are plainly simple. Indoor shading
makes a strong horizontal division between the pale top and dark underside; soft
shadows have visible repeated bands. The former jagged rim and tail diameter step
are no longer visible in this set.

Moonhart's cream muzzle, nose, large ears and branching antlers make its face and
species readable. Quarter views show a connected chest/neck and four planted
hooves. Lower legs and upper-leg attachments retain a toy-like construction.
Indoor/softbox frames 024–031 reveal mottled shading across the forehead, muzzle
and ear backs, and broad light/shadow bands; the surface is not uniformly smooth.
Antler branches remain blunt and visibly segmented at their junctions.

Cabin roof edges and wall/gable joins are clean. The open doorway shows consistent
floor planks; windows now read as cream four-pane frames with opaque blue-gray
panes. Remaining limits are the empty dark interior, featureless roof planes,
stippled shading beneath the softbox-lit eaves (028–029), and dotted narrow siding
seams from above indoors (027). The window panes do not reveal the interior.

Across all three subjects, golden hour, afterglow, overcast and rain substantially
lose face highlights, material separation and lower-body detail. Sunset strongly
warms their colors. These are visible shared-light readability limits; the suite
does not establish uniformly readable art across conditions.

`astra-second-wet-captures.json` indexes additional matched rain quarter pairs.
Astra inspected cabin, Moonhart and Cloud Ray with wetness 0 and 1; camera, light,
time, pause state, scene look and source/shader digests match within each pair.
Wet surfaces darken and gain restrained upper-edge highlights. Moonhart's wet
torso shows a particularly abrupt broad shading boundary, and both faces lose
contrast. Cabin wet siding seams become brighter against darker boards; its wet
pair does not show the doorway/windows. No wet geometry discontinuity was visible.

Root also exercised native Motion → flight and the timeline at requested 6.54 s
(actual 6.53336 s). Its actual indoor screenshot showed floor clearance, coherent
tail taper and readable eyes; capture evidence is indexed by
`astra-second-native-motion.json` in the native evidence directory. Earlier
Moonhart contact/hop/blink evidence remains in `astra-motion-captures.json`.
Root inspected flower condition frames 001, 005, 013, 019 and 027: noon/indoor
separate cream petals from green leaves/stem, with visible facets at macro scale
and severe shared darkness in low light. Other flower condition frames have not
been visually reviewed. No model, animation, lighting or baseline was edited
during this review.
