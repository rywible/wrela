# Authored night environment

Status: closed after bounded runtime-readability acceptance of 0334 v2,
September 13, 2026. Actual matched stage and populated destination retain
readable surfaces with a darker forward horizon; numerical checks pass. The
0306 profile meets this rolling-cache scene's budget, and v2 changes only scalar
constants on that path. This is not a global night mood or art baseline approval:
zenith still appears pale and overcast. No further shader layer is queued.

## Observed defect and rejected recipe

The actual `ear-groom-0130-established-return-b8a44122.png` shows a black sky
behind a weakly filled bench and terrain. The receipt reports clear/night time
0.103111148, solar altitude −12°, afterglow preset, exposure 1, and outdoor floor
(0.0063, 0.0098, 0.014). Published cache generation 4 was exact, age zero, with
matching requested/source signatures. Stale cache publication does not explain
this image. Camera was (11729.00390625, 7.51126289, −7169.21435547), yaw π/2,
pitch −0.08.

Root's matched Soundstage tree comparison is recorded in
`.build/sanctuary-native-20260912/night-readability-0130-tree-replay.json`:

| Actual PNG | Solar altitude | Observation and decision |
| --- | --- | --- |
| `night-tree-minus12-b84d14ae.png` | −12° | Black sky; green tree receives nearly directionless fill. |
| `night-tree-minus4-9516f096.png` | −4° | Strong orange sunset horizon and purple clouds. Rejected as constant night treatment. |

Both use azimuth −35°, coverage 0.42, density 0.8, haze 0.55, seed 1.68,
exposure 1 and the same floor, camera, geometry and shared look. Source digest
`d4dcee44f7554bab4096f997820d6b927cddabdbc5a35a15003a5e37c7b15eca`, shader digest
`2d3f3d0debd4e192e4df828a4f0720cd5fd86a4f4e3d00f249e4352689a3ed05`.
The PNG SHA-256 values are respectively
`5072dd893c00901979ea3c55552c8bd8865d8882e0e93036da0aaa35f4772a3b` and
`ac63a6773e21ee99a81a3dbb2a5e9c703e964d5db59958a392de8dcd3a307e1c`.
No climate altitude change was made.

## Bounded shared response

The current solar atmosphere remains the source of solar scattering and direct
sunlight. Previously the outdoor floor was added only to surface diffuse light.
The candidate interprets that existing RGB parameter as clear-sky upward
diffuse irradiance divided by π for an authored night environment, and adds the
corresponding radiance to sky, diffuse light, reflections and aerial haze.

For a unit direction d and cached cloud transmission T:

```
s(T) = 0.25 + 0.75 clamp(T, 0, 1)
q(d,T) = (0.1 + 1.05 clamp(d.y,0,1)) s(T), d.y >= 0
q(d,Tzenith) = 0.18 s(Tzenith),            d.y < 0
C = clamp(finite(outdoorAmbientFloor), 0, 0.25) / 0.8
Lnight(d) = C q(d,T)
```

Nonfinite transmission becomes zero; each nonfinite floor channel becomes zero.
Directions supplied to the helper are finite unit vectors. Indoor and finite
point-light rigs set C to zero. The lower hemisphere is an approximate uniform
ground bounce using zenith transmission. No new celestial emitter is claimed.

Lambertian reflection is albedo/π times incident irradiance, whose integral uses
the positive cosine to the surface normal. Consequently a cache storing E/π
multiplies linear albedo directly. This follows the normalization used by
[PBRT's diffuse reflection model](https://pbr-book.org/4ed/Reflection_Models/Diffuse_Reflection).
For clear transmission, upward E/π of current v2 q is
`2 ∫₀¹ (0.1+1.05y)y dy = 0.8`; hence the coefficient division by 0.8.
Downward E/π is 0.18. A horizontal normal receives
`0.05 + 0.7/π + 0.09 = 0.3628169203`. With spatially constant T these values all
scale by s(T). They are analytic expectations independent of the production
128-direction quadrature. Native-reviewed v1 used `q=.4+.6y`, the same clear
up/down energy, and horizontal E/π `0.2+0.4/π+0.09=0.4173239545`; its numerical
receipts below do not validate the new v2 horizontal distribution.

This does not extend the Hillaire transport algorithm to moonlight. The
[author's reference implementation](https://github.com/sebh/UnrealEngineSkyAtmosphere)
provides a real-time sky/atmosphere model and path-traced comparison; our solar
transport remains separate from this deliberately authored environment. The
candidate adds no stars, moon disk, lunar shadows, airglow simulation, atmospheric
night scattering, spatial cloud shadows or full scene GI. Cloud transmission
only screens this broad radiance, leaving a 25% minimum fill in opaque cloud
directions; it does not integrate lighting inside clouds.

## Existing alpha channel and publication contract

Only `Engine/FieldEngine/Resources/Surface.metal` night-related regions change:
the helpers following the atmosphere insertion marker, `sceneLightState`, the
ambient/reflection/haze portions of `shadeSurface`, `skyFragment`, and
`atmosphereIrradiance`. A standalone numerical kernel is added to that file.

Both the scene irradiance and clear `cloudAmbient` textures already have RGBA
storage. `atmosphereIrradiance` previously wrote alpha 1, and production consumers
used RGB only. Alpha now stores unit night E/π, using the same existing 128
directions and each sky sample's alpha transmission. RGB solar equations and
quadrature remain the same. The clear cloud ambient texture therefore also
contains a clear unit convolution in alpha. Cloud transport still reads only
its solar RGB.

The current source retains the original solar-only luminance meter in
`sceneLightState`, independent of view/subject orientation. The first 0237
candidate included clear night irradiance in this meter; actual native evidence
below rejected the resulting dark surface response. Its removal preserves the
existing bounded dark gain and lets authored night radiance affect image
brightness. The gain applies to the entire HDR scene; shared look and exposure
controls retain their equations and values. This is a deliberate adaptation
policy, not a simulation of human dark adaptation.

The floor coefficient remains a live uniform outside the sky signature. Its
fade does not restart cache construction. Existing cache staging, copies and
atomic publication carry alpha with RGB; `verifyPublishedCache` compares all
four components. Hot reload constructs a new Atmosphere before replacing the
pipeline set, so old alpha-1 caches are not reused by the new shader. Paused
exact captures and completed-cache verification retain their current paths.
Live sky reprojection versus cached diffuse timing remains the pre-existing
approximation; this change does not claim to eliminate that lag.

Zero floor uses the original solar RGB result with no night addition. The
repurposed unused alpha differs even then; rendered daytime RGB is the
regression criterion. Cave room output remains on its existing early/indoor
branches. Existing solar disk attenuation and direct-light equations remain.

## Verification and cost gate

The diagnostic-only `nightEnvironmentCheck` kernel takes two float4 records per
case in buffer 0: `(direction.xyz, transmission)`, then `(floor.rgb, outdoorFlag)`.
Buffer 1 receives `(nightRGB, unitRadiance)`; buffer 2 is uint case count. It is
never dispatched in live rendering. World-content's independent Swift verifier
owns CPU/Metal parity, zero/indoor/nonfinite cases, and irradiance energy checks
alongside the existing constant solar-sky test. Numerical implementation is in
`Tools/SoundstageKit/Authoring/FieldVerification.swift`.

No texture, render/compute pass, uniform layout, cache allocation or cache update
schedule is added. The irradiance pass reuses RGBA samples and adds scalar
arithmetic. Reflection/haze and sky consume alpha from their existing sample.
The below-horizon background adds a zenith-alpha lookup only with positive night
coefficient. Fragment ALU and possible register pressure are unmeasured until
root's native profile; zero added allocations is not a performance proof.

Root acceptance sequence:

1. Serialized renderer check: CPU/Metal helper parity, independent irradiance
   quadrature, constant-sky energy, zero-floor solar and indoor behavior.
2. Exact paused −12° Soundstage tree at the recorded inputs: sunward, away and
   zenith, then bench/animal/terrain at the exact returned-game camera. Inspect
   cool sky/cloud distinction, directional form and horizon separation; reject
   gray washout, excessively bright ground or a daytime/sunset appearance.
3. Repeat exact captures and verify aged published cache against its captured
   source time. Retain full generation/source metadata and shader fingerprints.
4. Short live 1920×1080 M4 run with wind, view motion and all ordinary cache
   updates included. GPU p95 target ≤16.7 ms; maximum presented interval target
   ≤33.4 ms. Report maxima and update frames separately without removing them.
   Overlapping GPU stage counters must not be summed.

Contact-shadow diagnostics are archived separately; no shadow renderer change
is part of this night experiment.

## 0237 numerical receipt

Sealed candidate `sanctuary-night-reeds-20260913-0237` has build source digest
`16210adf07e4b52e6587206ad858705a6d708a9e59d54bf455bb887956c47841` and Soundstage
binary digest `3c7cfc1c06cca4478519d912e3cc1a08800092e873082bcd7aff290a0baf796d`.
Root's `night-reeds-0237-renderer-check/receipt.json` records successful
`--check-renderer` on Apple M4 and verifies candidate hashes before/after.

- Four CPU night tests passed, 0 failures, 0.041 seconds.
- Production night helper CPU/Metal maximum absolute error: 5.9604645e−8,
  below the unchanged 2e−6 gate.
- Diffuse verifier reported combined maximum 0.006604582. Because solar RGB has
  its own <0.009 gate and night alpha has its own <0.0035 gate, this successful
  value is necessarily the solar maximum; night alpha passed but its exact
  maximum was not separately printed. Do not report 0.006604582 as alpha error.
- The shader compiled; existing creature deformation and groom checks also
  passed. No numerical shader repair or tolerance change was warranted.

Prepared exact study inputs and matched-view/live acceptance card are in
`.build/sanctuary-native-20260912/night-0237-acceptance/`. That packet includes
the unchanged original −12° saved study and a noon55°/zero-floor variant.
These checks establish implementation/energy consistency only; actual night
readability, daytime image parity and performance remain separate evidence.

## First 0237 actual image

Inspected `night-reeds-0237-stage-held/.soundstage/captures/`
`night-tree-0237-minus12-5f3f37a7.png` and its metadata, then re-opened the
historical −12° reference PNG. Candidate shader digest is
`4e8236246461a6c2cd4065324b9318fd4761ffe9b5c2678d32a6bae37a58ecca`.
Saved camera, tree source controls, complete scene look, −12° sky and floor
match the earlier study; source/binary provenance differs. Candidate generation
3 is exact paused, age zero, with matching signatures and sceneSunLeadsCache
false.

Cool sky openings and cloud silhouettes now appear without the rejected orange
sunset. However, leaf volume is weakly legible, the trunk is nearly black, and
the nearby stage floor is substantially darker than the old diffuse-only
reference. This is a concrete readability concern, not a numerical failure or
a stale-cache explanation. The new clear night input reduces the solar-only
meter gain, while directional/cloud weighting also reduces surface fill.
Those changes explain the direction of the result; their individual measured
contributions are not isolated by this image. No shader adjustment was made
from this first view. The returned bench/terrain view and other sky directions
are the next actionable evidence, before deciding whether the approximation
or its shared authoring inputs need revision.

## Resident rejection and minimal meter correction

Root's populated `night0237-return-resident-d413d6aa.png` confirms the same
failure in the destination: bench seat/back and terrain are nearly black,
with poor shape and contact readability. The earlier e6c46c70 image preceded
async population and is not used for this decision. The resident frame has
1,005,329 visible triangles, exact cache generation 2, age zero, matching
signatures and coherent scene sun. Camera and climate match the return above.

At −12° solar radiance is negligible. The added clear night meter term has
luminance `.2126*.0063 + .7152*.0098 + .0722*.014 = .00935814`, predicting gain
`.055/.00935814 ≈ 5.877` instead of the established dark clamp 24. When this
term dominates, increasing authored floor scales radiance and inversely scales
gain, nearly cancelling its intended brightness control. Cloud screening and
directional convolution further reduce fill relative to the old uniform floor.

A proposed whole-frame exposure-4 diagnostic was rejected atomically by the
existing production range guard (0.5...1.5). The guard was preserved; no bypass
or new exposure range was introduced. Root authorized the smallest source
experiment from the concrete images and meter math: restore the original
solar-only meter input. Only `sceneLightState`'s metered-radiance expression
changes. It restores the ability to author night brightness while keeping a
single scene-wide gain and directional environment. The expected gain ratio
is about 4.084 in this scene, but actual sky mood must still be inspected.

The q/C model, alpha convolution, solar altitude, source coefficient, cloud
screening, diffuse/reflection/haze/sky equations and numerical expectations
are unchanged. No field, geometry, per-object exposure, uniform layout,
allocation or additional pass is introduced. The next exact tree/campsite
images must reject excessive sky brightness or loss of night mood; live M4
profiling still includes normal cache updates. No build/test/GPU work was run
by this shader author for the correction.

## 0306 matched actual images: readability versus mood

Inspected actual quarter `night0306-minus12-quarter-4fa4ae54.png`, sunward
`night0306-minus12-sunward-ccaf0f2b.png`, and noon zero-floor
`night0306-noon-zero-f8132e3e.png` from
`bridge-review-0306-stage-held/.soundstage/captures`. The replay wrapper is
`bridge-review-0306-night-replay.json`. Source digest is
`1f97a657cf53fecbb0d085a798dd82439e5fe33d7c47d5f47c9a74528fa92e4b`; shader digest
`2446eb4d92cf329be3d6c40394f7c2a83bdba602dff82890a58746455cfbc941`.

Compared with 0237, quarter metadata is exactly equal for camera, sky, floor
and exposure, scene look, lighting preset, wetness, time, authored tree source,
layout, rig and saved wind state. Both have 162,400 visible triangles at
1920×1080 and no GPU errors. The candidate's Surface.metal differs from 0237
only in the solar-only meter expression and its comment. The repeated 0306
quarter capture `61843c6f` decodes to exactly equal RGB pixels. Its generation
2 is exact paused, age zero and sceneSunLeadsCache false.

Canopy leaves and volume are now legible, and the nearby ground grid is clear.
The trunk remains nearly black; it is also very dark in the noon image, so this
alone does not establish a defect in the night environment. The quarter sky is
substantially brighter; the sunward view's pale blue opening and slate clouds
read closer to cool overcast than to convincing night. No sunset orange returns.
This is an observed tradeoff, not night-mood approval or a new source decision.

Fixed display-RGB crop means quantify this comparison (8-bit channel values,
not segmented objects or linear radiometric energy):

| Crop, pixel bounds | 0237 RGB mean | 0306 RGB mean |
| --- | --- | --- |
| Canopy, (800,320)–(1100,480) | 4.55, 17.99, 7.65 | 22.19, 63.34, 31.72 |
| Trunk, (938,590)–(957,730) | 0.99, 0.98, 0.77 | 3.25, 3.17, 2.24 |
| Ground, (1300,700)–(1700,950) | 5.65, 9.50, 14.42 | 28.23, 41.24, 54.71 |
| Upper sky, (0,0)–(1920,270) | 9.48, 15.68, 22.50 | 40.68, 57.11, 73.55 |

The read-only pixel analysis is archived as `night-0306-matched-pixel-summary.json`.
The noon image is visually coherent, with daylight sky, leaf highlights and a
directional shadow; without a matched pre-change noon render it does not prove
old/new daylight pixel parity. No further shader change was made from these
views. Root's corrected populated campsite and short live profile remain the
next decision gates.

## 0306 populated campsite and live profile

Actual `night0306-resident-return-c6db530b.png` is the fully populated return,
referenced by `bridge-review-0306-return-resident-capture.json`. It matches 0237
resident d413d6aa exactly in camera, climate, floor, sky, scene look, time,
expedition, living-world state, visible triangles and streamed detail chunk IDs.
Streaming is settled in both; the only streaming-metadata differences are three
CPU preparation/generation timings. Bench seat/back are now distinguishable,
but the object remains a dark plain block and the sky reads cold overcast.
Source seating/contact facts are unchanged; no shadow or model change follows
from this shared-lighting comparison.

Root's actual `night0306-return-live-20260913-032537.json` reports 8.38991 seconds
on Apple M4 at 1920×1080, 4× MSAA, thermal state 0, low-power mode false, a fixed
camera and 1,005,329 visible/434,608 shadow triangles. Allocated Metal memory
stays 1,217,609,728 bytes. The profile has 510 timing frames and 14 state samples;
no sampled GPU error or sceneSunLeadsCache flag is present.

| Metric | Result |
| --- | --- |
| GPU median / p95 / maximum | 9.442 / 12.033 / 13.507 ms |
| Actual presented maximum interval | 16.666667 ms |
| Submitted frame interval maximum | 20.232 ms |
| Submitted / presented FPS | 60.072 / 60.072 |

The GPU maximum occurs in a frame labelled `candidate.lighting`; this is a whole
frame classification, not an isolated lighting-pass duration. Counters were
disabled and stage intervals are not summed. Included phases are sky, lighting,
air, ambient, irradiance, clear, multiple scattering, transmittance, waiting and
weather. Publication count rises 2→3 during the interval. Active signature and
generation remain 2: this exercises rolling updates/publication of the same
night source, not a different climate key or full density reconstruction.
Weather seconds/water remain unchanged in this short sample despite a weather
scheduling phase. Maximum final cache age is approximately 11.567 seconds;
this is retained, not treated as fresh-cloud transport.

This scene meets GPU p95 ≤16.7 ms and maximum presented interval ≤33.4 ms with
normal rolling updates included. It does not certify the heavier established
journey, arbitrary climate transitions, or a complete new-key candidate. The
0306 profile does not measure the yet-unrendered v2 look.

## v2: preserve clear upward fill, darken the forward horizon

The next bounded candidate changes only the shared upper-hemisphere angular
function from `.4+.6y` to `.1+1.05y`. Clear upward E/π remains .8 and the saved
floor coefficient remains `F/.8`. Lower bounce .18, cloud screening, solar-only
meter, solar equations and all sky/diffuse/reflection/haze consumers remain.
The perceptual purpose is a darker forward sky relative to the lit terrain;
a global gain change would move them together without changing this contrast.
No individual object is brightened and no lighting layer is added.

In clear directions the horizon radiance falls 75%, radiance at y=.5 falls
10.7%, and zenith radiance rises 15%. Clear irradiance on vertical surfaces
falls from .4173239545 to .3628169203 (13.1%), so the bench's vertical back may
become darker. Under nonuniform cloud transmission even upward irradiance can
change because angular weights move toward zenith; only clear or spatially
constant transmission has the exact preserved-energy guarantee. The zenith
increase is another explicit visual risk. No claim of a physical night emitter
or human adaptation is introduced.

The diagnostic kernel API stays the same. Independent CPU reference and analytic
assertions must use the stated v2 function and preserve their error tolerances;
passing the old v1 receipt is insufficient. Root's next matched tree, resident
bench, sunward and zenith images must show improved night contrast without
destroying back/leg visibility or producing a bright zenith. Noon with floor
zero remains on the same source expression. There are no added branches,
texture fetches, loop iterations, passes or allocations; only scalar constants
change in the existing function. Repeat live profiling only if subsequent
changes alter that cost path or new performance evidence warrants it.

## 0334 v2 actual stage gate

Root's sealed 0334 four CPU tests and Apple M4 renderer check passed unchanged
tolerances: night helper error 5.9604645e−8, combined diffuse maximum .006604582
(solar RGB; alpha separately passes <.0035). The independent CPU integration
initially retained an old v1 literal; its failing horizontal-energy assertion
caught that mistake before this candidate. The reference formula was corrected
to v2, without widening a tolerance or changing the production distribution.

Actual images from `bridge-reeds-night-0334-stage-replay.json` were inspected:
quarter `8be662d7`, quarter repeat `d906bb82`, sunward `cfb4b8aa`, zenith `44f01d8a`
and noon zero-floor `438c03c8`. Source digest is
`434347b8a7a71bfd638703d7ae923c6651443f145228b2b604c6a877434323ea`; shader digest
`ebbce1c44ef06bf242281c5b8577c87d713ed67972f0a45f03ec05db06a3f202`.

Camera, sky, floor/exposure, scene look, time, lighting, visible triangles,
wetness, authored source, rig, layout and wind state equal 0306. Repeated quarter
RGB pixels are exactly equal. The cache is generation 2, exact paused, age zero,
with coherent scene sun. Studio near-plane policy also changed in this
candidate, independently of the night shader, so geometry-depth differences
must remain separate from the lighting comparison.

The quarter horizon is now clearly darker while the canopy and floor retain
readable form. The trunk is still very dark. The sunward near-horizon area also
improves, but the zenith view is pale blue with broad soft cloud edges and still
reads as overcast; this does not establish an accepted night mood. The next
destination gate must check that the vertical bench back and legs have retained
enough visibility after angular redistribution. No further shader edit was made.

Fixed display-RGB8 crops from 0306→0334 quantify the observed direction:

| Region | 0306 RGB mean | 0334 RGB mean |
| --- | --- | --- |
| Horizon (0,310)–(650,350) | 85.88,112.85,136.72 | 31.66,45.19,59.53 |
| Upper sky (0,0)–(1920,270) | 40.68,57.11,73.55 | 24.33,35.80,47.64 |
| Canopy (800,320)–(1100,480) | 22.19,63.34,31.72 | 19.40,58.85,27.84 |
| Ground (1300,700)–(1700,950) | 28.23,41.24,54.71 | 29.34,42.66,56.47 |

These are image crops, not segmented or radiometric measurements. Noon's full
image mean absolute RGB8 difference is (.00170,.00205,.00139), with maximum
channel difference 46 and changed-pixel bounds (727,298)–(1159,545), confined to
the canopy. That is consistent with the separate near-plane/depth change but
does not by itself prove causation or bit-exact daytime parity. The noon sky
and ground outside that region are unchanged in this comparison. Full small
analysis receipt: `night-0334-matched-pixel-summary.json`.

## Destination acceptance and experiment closure

Inspected actual `night0334-resident-return-5368a9c9.png` and metadata, referenced
by `bridge-reeds-night-0334-return-resident-capture.json` (frame 3643). Bench
back, seat and both legs remain discernible; the forward horizon is appreciably
darker than 0306, and nearby terrain stays readable. The overall scene and
bench still look plain. This supports the bounded readability objective, not a
global art baseline or a claim of convincing night transport.

Camera, climate/floor, sky, scene look, time, expedition, living-world state
and streamed detail chunk IDs equal 0306 exactly. Both are fully resident:
70,581,608 source bytes, 9 terrain chunks and 81 vegetation chunks, complete
vegetation, no pending publication/upload or queued work. Streaming metadata
differs only in three CPU generation/preparation timings. Visible triangles
are 988,945 versus 1,005,329 in 0306, a difference of 16,384; the comparison is
therefore not a bit-exact geometry workload claim. The difference is retained
without assigning an unsupported cause or expanding this lighting task.

The current cache is exact paused generation 2, age zero, requested/published
signatures equal and sceneSunLeadsCache false; no GPU errors are reported.
Root's numerical checks and exact repeat stage frames remain authoritative.
The 0306 rolling-cache performance receipt is retained with its stated workload
limits; no new v2 timing claim replaces it. Only scalar angular coefficients
changed between those versions, with no extra runtime work path introduced.

One concrete night art issue remains: zenith's broad pale-blue opening and soft
cloud edges read as overcast daylight. A later production test should revisit
this same saved campsite at the exact −12° night inputs, use ordinary camera
controls to look from the bench toward zenith and back, and inspect both frames
under the same shared look and floor. Any later correction must improve the
look-up mood while retaining bench-back/leg and terrain readability when the
view returns; it must not use object exposure compensation. The task is a
future art gate, not permission to add another lighting subsystem now.

This experiment queue is closed. Contact-shadow research remains closed too;
no additional shader authoring, build or GPU run was performed for closure.
