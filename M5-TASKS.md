# M5 tasks (deleted when the milestone closes)

Scope: #28 ("The clearing, specified", § numbers). Criteria: #51 (AC numbers).

## A. Language and hosts (§10, AC10)
- [x] A1 `discard`: spike 15's commit, conformance cases (fragment ok; E0607 in a kernel, a vertex shader, CPU code), a diagnostics golden, language.md §12
- [x] A2 Depth state per draw (compare: less, less-equal, equal, greater, always; writes on or off), build-time constants as cull and depth bias; manifest; both hosts; conformance; renderer.rs frames in both hosts
- [x] A3 Frame timing: each timed pass's start and end in both hosts; a frame's span; the serial mode (passes one at a time); a pass's own time against the pass alone within 10%
- [x] A4 std's ellipsoid without the zero at its centre (spike 13's case); M4's parity tests hold

## B. Terrain (§3, AC3)
- [ ] B1 `engine::terrain`: a terrain field, its clipmap cooked on the GPU (height, normal, material), its raycasts on the CPU
- [ ] B2 The clipmap's mesh: nested rings, morphing, skirts; the vertex shader reads only the clipmap
- [ ] B3 Tests: WGSL reads no field; no cracks; accuracy; one terrain (raycast against clipmap)

## C. Vegetation (§4, §9, AC4)
- [ ] C1 `engine::plant`: limbs along curves, crowns of clumps; its field (lens); cooked cards (normal, depth), card levels
- [ ] C2 Cards drawn: depth prepass (discard), shading with equal depth, no writes; card levels by distance
- [ ] C3 Grass placed on the GPU round the eye (Append, indirect draw), nested levels so density is continuous; flowers with it
- [ ] C4 Impostors (octahedral, colour, normal, depth) and density volumes per kind; matched levels
- [ ] C5 Wind in grass and leaves (time and place only)
- [ ] C6 Stones: fields realized to meshes at load, cooked normals
- [ ] C7 Tests: coverage along the path; shading count per pixel (atomic, test build); levels match; wind; WGSL of stones

## D. Light and sky (§5, §6, AC5, AC6)
- [ ] D1 Static cascades cached; dynamic shadow round the creature; darker of both; rotated disc taps
- [ ] D2 Probes baked at load (sky light, occlusion, bounce) from a cooked occluder volume; brute-force reference test
- [ ] D3 Haze by height
- [ ] D4 Clouds cooked at load in strips (≤ 100 ms each); sky pass reads the texture; cloud shadow map

## E. Frame, TAA and the look (§2, §7, AC1, AC7)
- [ ] E1 Frame at 960×540 into Rgba16Float + depth, tags in alpha; Halton jitter
- [ ] E2 Motion target (the creature, from this frame's palette and the last); camera reprojection for the rest
- [ ] E3 TAA and upscaling to 1080p: reprojection, neighbourhood clamping, sharpening
- [ ] E4 The look at 1080p: gouache dabs anchored on surfaces; the creature's cel bands and lines; bloom; tone curve
- [ ] E5 Tests: upscaled against native; flicker (warp error); ghosting

## F. The creature (§8, AC8)
- [ ] F1 Gait from a path along the track; IK foot planting from terrain raycasts in the tick; pelvis lowered
- [ ] F2 Springs per bone (head, tail), stepped with the tick
- [ ] F3 Cooked normals and channels in the mesh; cel shading
- [ ] F4 Tests: no slide, feet on terrain, leg lengths, springs' energy decays

## G. Authoring (§9, AC9)
- [ ] G1 Hot reload, both hosts: literal edits (≤ 0.5 s), structural edits (≤ 3 s), no restart
- [ ] G2 The great tree as a plant in its own package; the lens draws, probes and drags it
- [ ] G3 The art-directed round: the great tree and the creature authored with the lens; edit shares
- [ ] G4 #31's M3 friction met in the round: fixed, or noted in #31

## H. The clearing and its measures (§1, §11, AC2, AC11)
- [ ] H1 The clearing's content (examples/clearing): meadow, track, rise, plateau's edge, valley, hills, mountain, forest wall, gap, boulders, flowers, cumulus, sun at 24°
- [ ] H2 The 60 s camera path (test input script)
- [ ] H3 Paced runs in Chrome: 0 of 3,600 frames over 16.7 ms, three runs; per-system slices
- [ ] H4 Measured: time to first frame, bytes before it, GPU memory
- [ ] H5 check.sh runs every test; WGSL budgets cover the clearing; speed of check and build; runtime size

## I. The record (AC12, AC13)
- [ ] I1 language.md: discard, depth state, frame timing
- [ ] I2 vision.md milestones table; #26 rows
- [ ] I3 Agent syntax test (AC12), into #26
- [ ] I4 Retrospective on #51 with spike 15's numbers beside M5's
- [ ] I5 Delete this file
