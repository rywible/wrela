# Sanctuary autonomous build progress

Updated September 12, 2026, during the continuing native implementation pass. **The complete game is not finished.** The current
source integrates the living-world direction from SANCTUARY_GAME.md. Native
appearance, usability and M4 1080p performance are not yet accepted.

## Scale and implemented behavior

The user rejected 1.52 km and requested larger than the intermediate 9 km plan.
The world now spans **32 × 32 km (±16,000 metres)**. All eleven environments have
terrain, water/ground treatment, procedural feature plans, landmarks and wildlife.
Local detail streams in 512 m chunks; the full-world coarse mesh supplies distant
landforms. Flight traverses deep water; walking/riding reject deep water, and
landing requires shore. Scale is not evidence of sufficient density or pacing.

- Ordinary native Sanctuary uses indirect typed animal interaction, nature and
  construction controls, observation/journal, walking/riding/flying and persistent
  individual relationships. Legacy rescue fixtures remain explicit test cases.
- A deterministic 25-actor roster spans all biomes. Preferences, trust, encounter
  memory and companion willingness persist. Nearby actors use full brains;
  distant actors use coarse updates. Follow reanchors across regions. Suitable
  nearby player patches attract up to three animals within 480 m, using production
  movement and preserving family links; removing the patch sends them back. Two authored
  young grow during play, with no offline aging or death loop.
- Animals create persistent dams, nests, resting beds and seed caches. Edits can
  displace structures; removing edits rebuilds the same identity. Dam collision
  and a local pool are gameplay approximations, not volume-conserving hydrology.
- Terrain raise/lower/level, flowers/groves/reeds/shallow water and undo compose
  through the same source queries as collision. Adaptive local tessellation
  refines sculpt areas to one metre. Tool reach/radius/strength are player controls.
- Seven construction primitives support rotation, scale, collision, walking
  surfaces and undo. The authored starting cabin has a doorway. Persistent user
  construction and nature edits remain bounded to 128 entries per registry.
- Version 3 saves preserve legacy reads and the original Frostling identity.
  Candidate edits save before commit; disk failure rejects atomically. Mounted
  camera/actor positions and autosave next-tick state have CPU regression tests.
- Strong, willing companions can move source boulders in two-metre increments,
  with collision/obstruction checks, persisted helper provenance and undo. Sparse
  offsets are limited to 32 m cumulative travel and 128 boulders. The current
  shift is immediate; a convincing pushing/contact animation is still absent.
- Streamed feature transforms, natural-water omission and primitive choice now
  share one resolved source for rendering/collision. Terrain edit/undo commits
  corrected player elevation in the same save transaction; flight save restoration
  and live movement use identical edited/animal-pool water clearance.
- Accepted external creature decisions are typed, validated against relevant
  facts and fresh range/visibility, persisted, and replayed without a model call.
  The optional native async coordinator is disabled by default. Native experiment
  recognized four clear paraphrases and rejected stale/duplicate decisions with
  exact replay, but misclassified contradictory text. Authored text remains immediate.
- SanctuaryClimate.swift defines a deterministic 40-minute play-time day/weather
  score. A new Experience candidate connects fixed named sky/weather states and
  wetness to saved play time, preserving manual study overrides. Native review is
  pending; transitions may hitch/be abrupt. Wind amplitude remains decoupling work.
- New species, constructions and ecological structures use procedural sources
  registered in Soundstage. These are provisional silhouettes and motions;
  no new visual baseline has been accepted.

## Verified checkpoint evidence

The following three successful reports share source fingerprint
`1a46c14644cb60db42119a628c2523773d9350ae70fc1027067739c00e30877d`
at start and finish:

- Full quick suite: **289 tests**, architecture/policy checks and eight production
  smoke scenarios passed. `.build/test-runs/20260912-155445-quick-d5527/index.html`.
- All four living scenarios passed: requests, habitat/building, riding and flying.
  `.build/test-runs/20260912-155703-run-5685d/index.html`.
- CPU performance: `.build/test-runs/20260912-155714-perf-d1e4a/index.html`.
  Five samples of the actual 25-actor population's **60-tick workload**: median
  **7.99 ms**, p95/max **8.10 ms**. These are per-workload timings, not native frame
  times. M4/16 GB/macOS 26.6.2; the other Soundstage session was open. No GPU,
  clouds, wind caches, chunk-mesh uploads or model activity is included.

`./scripts/build Sanctuary` passed and signed `.build/Sanctuary.app`. The occupied
Soundstage bundle was not overwritten. Retained build log, aggregate source/app
hashes, shader hashes and report links:
`.build/sanctuary-checkpoint-20260912/manifest.json`.

Native host attempt `.build/test-runs/20260912-155846-host-c2a6d/index.html` failed
**before native launch**, because another Soundstage process (35696) was active.
This earlier blocked attempt is historical; native work has since resumed below. No appearance baseline is accepted.

Regression coverage includes habitat family arrival/removal/replay, boulder helper
capability/control/undo/reopen, shared transformed source collision/water omission,
external decision replay and stale/duplicate/visibility/disk failure, edited-pool
flight restoration, terrain edit camera save/reopen, relationships, mounts, and
legacy saves. The initial follow-up regression in local habitat work was fixed;
its failed focused log is preserved in the checkpoint folder for context.

Earlier report history remains under `.build/test-runs/`: 132956-quick-1d283,
151853-quick-b8067, 153423-quick-597b5 and 153913-run-7334a. The
153125-quick-c42af run is invalid evidence because a script changed during it.
Those earlier fingerprints do not validate the final additions.

Foundation Models native experiment ran in an isolated save/session. Four clear
paraphrases classified correctly (cold 2878 ms, warm 410–420 ms); contradictory
text incorrectly became come. Unsupported text produced noMatch. Exact accepted
record replay, atomic duplicate rejection and stale world revision rejection passed.
It remains disabled by default. App RSS excludes system model-service memory; short
frame windows show GPU spikes up to 19.48 ms and do not isolate model causality. See `studies/SANCTUARY_INTERPRETER.md` and
`studies/SANCTUARY_LIVING_WORLD.md` for decisions and limitations.

## Current owners and next dependencies

Lead owns builds, tests, native/GPU/model scheduling and reports. Astra alone owns
ALL 3D models and animation. Its second candidate (LivingWorldPresentation.swift,
SanctuaryProceduralCraft.swift) refines cabin planar surfaces/materials, Ray tail/
flight inspection clearance and flower anatomy/color. Host_controls owns the frozen
Experience climate/audio adapter; game_scenarios owns eleven frozen exploration
routes in SanctuaryTests; nature_building owns construction selection/update/removal/
undo and history consistency work. Root owns new procedural audio synthesis/call
presentation and its tests, plus two landmark observations. No build runs while
these candidates are changing; all native apps are currently closed. Interpreter
worker finished the bounded experiment report. Preserve pre-existing source/saves.

## Native iteration evidence (not final acceptance)

- Saved the authorized Vesper study, closed that session and built Soundstage.
  Initial actual captures under `.build/sanctuary-native-20260912/stage/` show
  Frostling's soft face intact; Moonhart's disconnected anatomy, Cloud Ray's jagged
  rim and cabin roof/wall mismatch drove the Astra revision.
- Exercised native Frostling Motion controls and captured the hop strip in
  `stage/workspace/.soundstage/studies/authoring-20260912-162038-cb29b4/`.
  Inspected side frames 012/018/023. HTML was read from disk; interactive local-file
  browser viewing was rejected by automatic browser policy and was not bypassed.
- Native Sanctuary typing/greeting and action-picker flower planting were exercised.
  Initial greeting prompt at 6.32 m exposed a 6 m action/8 m prompt inconsistency;
  source now shares an 8 m request range, with a boundary/rejection regression.
  Production semantic movement was used; OS held-WASD movement remains unverified.
- Native host report `20260912-162138-host-c14b4` launched but failed because
  `natureControl` reused request-envelope `id`. Source/script now use `controlID`;
  native recheck now passes in `20260912-163558-host-dfecd` (source before
  the second art/climate/construction-edit/audio candidate).
- First live native M4 1920×1080 sample (6.60 seconds): presented/submitted 60.01 FPS,
  GPU median 13.98 ms, p95 14.38, max 14.82; frame interval p95 17.64, max 18.89 ms;
  CPU simulation p95 1.48, max 6.27 ms. Sky lighting/air/sky update phases occurred,
  clouds/wind/ecology stayed live, thermal nominal and low-power off. Fixed opening
  view only: no chunk crossing, timed edit or model activity. Report:
  `game-first-look/runtime/profiles/initial-native-before-art-20260912-162723.json`
  beneath `.build/sanctuary-native-20260912/`. This measures the prior compiled art,
  source digest `6fc254826fccb7fea6117e159f15b89132ce7fa06d0f1c652b2f37f55b648194`,
  shader `c32257ce930b722659fe1ab5907751485051575d0b5022447662912975dfae1b`.
  The complete representative performance target is not established.

## Blocker and remaining requirements

The user explicitly authorized full computer operation and saving/closing the
Vesper session. Its working study is preserved at
`.build/sanctuary-native-20260912/vesper-before-sanctuary.json`. The old session was
quit cleanly, Soundstage rebuilt, and root now owns an isolated native GPU session.
No further confirmation is needed for this authorized native work. Initial actual
captures show Frostling's soft face intact, but Moonhart anatomy, Cloud Ray rim and
cabin roof/wall junction require visual improvement. Astra owns the model/animation revisions; root owns all native/GPU/build activity.

Remaining full-game work includes convincing companion assistance motion, broader
habitat recruitment/recovery play, day/night/weather presentation integration, richer
discovery/recovery aspirations,
regional density and route playtesting, personal building preview/edit usability,
expressive species art and motion, calls/audio, and the optional model experiment. Accepted decision recording is implemented;
classification quality and live impact remain unmeasured. Authored young exist; runtime births do not. The
32 km terrain/detail seams, synchronous streaming upload hitches, biome aesthetics,
walking/riding/flying feel and construction/water appearance need actual native
review. No test establishes enjoyment or complete-game quality.

## Replay and launch

Use isolated harness roots and named expedition slots; preserve player defaults.

```sh
./scripts/test quick
./scripts/test run --game sanctuary --tag living
./scripts/test perf --game sanctuary --kind cpu --samples 5
./scripts/build Sanctuary
./scripts/test host --game sanctuary
./scripts/test render --game sanctuary
```

A concrete retained replay:

```sh
./scripts/test replay .build/test-runs/20260912-155703-run-5685d/sanctuary-flying-movement.json
```

Replay a retained scenario JSON from its report with `./scripts/test replay PATH`.
Inspect failure images with `./scripts/test inspect PATH --step N`; inspect creature
records with `--stage`. Native work requires exclusive GPU lease. Use Soundstage
project sanctuary and catalog before selecting subjects. Never accept baselines
silently. The report and pinned inputs, not command success alone, are evidence.

## Latest validated art/protocol checkpoint and pending integration

- Focused quick passed: `.build/test-runs/20260912-163134-quick-30b1b/index.html`.
- Native host passed: `.build/test-runs/20260912-163558-host-dfecd/index.html`.
- Creature authoring validation passed:
  `.build/test-runs/20260912-163658-authoring-ef9e4/index.html`.
- Actual Moonhart/Cloud Ray/cabin and shared-look review, limitations, source/shader
  fingerprints and replay commands: `studies/SANCTUARY_NATIVE_ART.md`.
- On-device results and raw study/replay script: `studies/SANCTUARY_INTERPRETER.md`
  and `.build/sanctuary-native-20260912/interpreter-study.json`.

These reports precede the pending second art, climate, audio and construction edit
candidates. Next: full quick (shared audio change), fix any compile/test failures,
eleven CPU exploration routes, serial Sanctuary/Soundstage builds, native art review,
local biome route captures and construction editing/undo/save/reopen through native
controls. Measure live transition/chunk/edit spikes separately from steady frames.
Quiet tonal creature cues have source but no auditory acceptance evidence yet.


## Shore/placement integration checkpoint — September 12

Full game remains incomplete. The shared workspace and player saves are preserved.
Root owns builds, native/GPU interaction and Experience/audio wiring. All source is
currently frozen for checks; world_content completed the Content elevation/shore
fixes, nature_building completed finish-editing, game_scenarios corrected route
fixtures, and Astra completed presentation integration. Astra alone authors all
3D models and animations; its current work is read-only image review plus the art report.
Host_controls is diagnosing global low-light readability read-only.

Implemented candidates now connect creature placement, perception and call positions
through `SanctuaryWorld.elevation(for:at:)`. Ocean Cloud Rays use the higher of water
and ground plus their flight clearance; previous terrain-only placement submerged
them by about 20 m. Species eye offsets replace a universal 0.8 m visibility target.
Moonlake uses one shared ellipse for water, bed shaping and a dry shore discovery
band, preserving its center and saved landmark ID. Construction now has an ordinary
Finish editing control with atomic saved deselection.

Latest completed full quick: 299 tests passed, report
`.build/test-runs/20260912-165626-quick-9127e/index.html`. This predates these shore/
placement/finish fixes. Debug contract checks exclude the eleven long exploration
routes; they remain required release and native checks, documented in TESTING.md.
The first release exploration run passed four and failed seven:
`.build/test-runs/20260912-165821-run-b20cf/index.html`. It exposed wrong companion
fixtures, obstructed endpoint routes, underwater tidepool placement and the actual
water/creature-height/discovery bugs above. Corrected routes query production solids
and water at setup and still execute production movement. Their recheck is pending.

Second Astra cabin/flower/ray art compiled successfully for both apps. Four complete
32-image condition studies and explicit wetness 0/1 pairs are retained under
`.build/sanctuary-native-20260912/astra-second-review/`; manifests are
`astra-second-captures.json` and `astra-second-wet-captures.json` in the native root.
Rain preset alone leaves wetness zero; only the explicit pairs establish wet/dry.
Actual images confirm repaired cabin joins, readable flower silhouette, smoother
ray tail and clear studio floor at flight 6.533 seconds. Native Motion dropdown and
timeline were exercised; capture manifest `astra-second-native-motion.json`.
Golden/overcast/afterglow are too dark across multiple subjects. No baseline accepted.

On-device experiment is complete in `studies/SANCTUARY_INTERPRETER.md`: authored
fallback immediate; four paraphrases classified correctly (cold 2878 ms, warm
410–420 ms), unsupported request rejected, contradictory request incorrectly accepted.
Recorded accepted decision replay matched exactly; duplicate/stale rejection was
atomic. Keep the interpreter disabled by default. Tonal audio has source/unit checks
but no auditory acceptance evidence. Climate is connected in source with saved-time
coarse sky states; abrupt transitions, night readability and update hitches remain.

Next: focused quick and release exploration; fix failures; close owned Soundstage,
rebuild Sanctuary, run native biome render checks, play construction/typed interactions/
movement/save/reopen and climate. Measure ordinary live plus transition/chunk/edit
spikes at 1080p. Continue relationships, ecology, route density and construction
usability against full specification. Do not treat local 20 m biome fixtures as
proof that inter-biome routes or the complete 32 km world are enjoyable.


### Follow-up defects and current test queue

Focused game quick passed with shared elevation and finish-editing:
`.build/test-runs/20260912-171030-quick-5cfae/index.html`.
Exploration fixture validation then found the entire tidepool landmark/actor habitat
roughly 10 m below sea level (`171121-run-140a6`, diagnostic rerun
`171238-run-5381d`). Terrain now supplies a blended intertidal shelf and shallow
pools, with tests for resident homes, the local route, offshore depth and landward/
seaward boundary continuity. These new tests have not yet run. Ocean fixture is
being corrected to begin explicitly mounted, because a walking-on-water fixture
cannot restore through the production snapshot contract.

Root's 12-image shared ambient 1.08/1.6 experiment is retained at
`.build/sanctuary-native-20260912/global-ambient-experiment.json`; actual golden
flower pair and overcast cabin still lack adequate readability. Published art
direction is unchanged. Host_controls now owns a bounded ambient-floor candidate
across GameLighting, FrameComposer, Surface shader, Experience, Soundstage renderer/
bridge/study serialization and CLI. This is authored night fill, not lunar lighting
or GI; native calibration and performance/replay validation are still required.
Root owns the GPU lease; the previous Soundstage has been closed cleanly. No app
runs in the background at this checkpoint. All other source workers are frozen
except the ocean fixture correction. Wildlife is assessing one optional recovery
loop read-only; Astra source remains frozen after the documented actual-image review.

The native host validation script now exercises bench selection, turn, resize,
move, finish, remove/undo, exact named-slot reopen and atomic unselected-remove
rejection. It has static syntax validation only so far. Next integrated pass must
include full quick (shared shader/host changes), eleven release routes, serial
Soundstage/Sanctuary builds, renderer numerical verification, native lighting and
construction/route checks, then live climate/stream/edit performance.


### Integrated results and native night study

Full quick passed **313 tests** and both game smoke groups:
`.build/test-runs/20260912-172212-quick-89231/index.html` (67.2 s unit run).
Release exploration is now **10/11 passing**:
`.build/test-runs/20260912-172534-run-25a0e/index.html`. Ocean fixture now explicitly
stages trust beside the water-supported Ray and records a valid mounted start;
it tests 24 m local flight, not a reachable walking encounter at sea. Earlier failed
setup artifacts are retained (`172357-run-1efdc`, `172454-run-99403`).

The remaining Alpine failure has an exact source cause: the saved Cloudstepper
focus lies inside the named spire's ellipsoid (normalized radius squared 0.6515).
Facing and terrain checks pass. World_content is offsetting the shared rendered/
collision feature while retaining the landmark and actor coordinates, with tests
for capsule/eye clearance and the production sightline. No creature model changes.

Soundstage and Sanctuary builds passed. Renderer numerical check passed on Apple M4:
`.build/sanctuary-native-20260912/night-renderer-check/report.json` (diffuse error
0.006604582, creature deformation 3.59e-7, groom coverage 4.36e-5).
Night study reports and exact recipes: `night-floor-stage-study.json` and
`night-floor-035-captures.json` in the native root. Invalid floor rejected atomically;
restoring the study reproduced the exact document. Root inspected actual Moonhart
and cabin images. Zero fill is fully black; the initial full fill looks daylit and
flat; 35% preserves dark surroundings with readable shapes. Experience source now
uses provisional RGB (0.0063, 0.0098, 0.014), pending rebuilt native game review.
The fill has no cast shadows, moon or stars and still gives rather flat form.
No baseline was approved. Soundstage is now closed; no render app remains running.

Next: validate the Alpine placement, rebuild the changed game, native host editing
and all biome renders, run `run-climate-study.py` for day states/manual hold/reopen/
live boundary evidence. Longer travel, dense living routes and richer recovery
remain incomplete. Also review a Canopy Glider released over water: only Cloud Ray
currently uses water-supported elevation, so companion switching may expose the
same submerged-root defect in other flying species.


### Native integration checkpoint — terrain/water blockers and research

Alpine collision repair passed three focused tests (`/tmp/sanctuary-alpine-tests2.log`)
and all eleven CPU exploration scenarios now pass:
`.build/test-runs/20260912-173151-run-322c1/index.html`.
Native host validation passed 21 checks, including construction edit/finish/remove/
undo, exact named-slot reopen, and rejection preservation:
`.build/test-runs/20260912-173243-host-09d06/index.html`.
All eleven native biome state checks passed in
`.build/test-runs/20260912-173345-render-91bd3/index.html`, but **visual acceptance
failed**. Actual PNG inspection found Alpine camera-covering far terrain, rectangular
stepped shorelines, sparse generic forests, and an obstructive mounted Ray view.
No baselines were approved. State checks do not establish visual quality.

The far grid is 444.444 m. At the Alpine capture, its interpolated surface is
129.90848 m versus exact ground 122.23649 m and camera 124.75185 m. It must exclude
active detail chunks and stitch boundaries. Local water center-filled 12.8 m quads
inflate shorelines and step heights; a continuous production signed-water field
and clipped mesh are underway. World span remains 32×32 km; current visual density
and long-distance presentation do not yet fulfill the desired grandeur.

Root inspected native morning/golden/night captures in
`.build/sanctuary-native-20260912/climate-native-study.json`. Golden is too dark;
night fill reads flat and shadowless against a black sky. Save/reopen climate is
exact; manual noon holds through ticks. The six-second live morning→noon boundary
at 1920×1080 on M4 had GPU median 13.646 ms, p95 14.400 ms, max 203.199 ms, and
presented interval max 199.999 ms. This includes the automatic cache rebuild and
is a performance failure despite steady frames. Source digest
`aee30bf36f9fcda136166aab2151eed3bb96cb1afccb7df01aa9c1950f46c7c1`, shader
`7f55536414b10c04dcf7a28ba822768356f68327da4bec5fa21b3d9c0b6d08d7`.
Replay script: `WRELA_CONTROL_ROOT=<isolated-runtime> python3
.build/sanctuary-native-20260912/run-climate-study.py`.

User reiterated research/reusable systems and AAA quality. This is a target,
not an achieved label. Bounded research now addresses amortized sky transitions
and signed water boundaries; experiments must validate source contracts, seams,
replay, live cost and actual appearance. Only Astra authors 3D geometry/animation.

Current owners: Astra `BiomeWorldPresentation.swift`, `World.swift`; world_content
`Terrain.swift`, new signed-water source/tests/report; wildlife population rare
habitat facts/tests; nature_building creature elevation/tests (timestamp foundation
is frozen); host_controls sky-transition research report only; root native/GPU,
GameHost Application (fixing duplicated typed submission), Experience and integration.
Other workers are frozen. One root-owned Sanctuary process is currently running in
isolated `climate-controls/runtime`, lease session 51107; it is the pre-change
compiled baseline. Quit this owned session before rebuild. Default saves untouched.
Next: finish native input evidence, close app, integrate source candidates and run
checks; inspect repaired Alpine/shores and stream boundaries; animate saved-time
nature events with Astra; resolve cache hitch, night readability, dense distinct
biomes, mounted view and longer connected play loops.


### Field/cache candidate: checks and first vegetation rejection

Full quick now passes **336 tests** and both game smoke groups:
`.build/test-runs/20260912-180359-quick-b1de6/index.html`.
The preceding `175948-quick-1f4df` found five assertions: one outdated center-only
camera fixture and four real dam recovery failures. The world starts habitat work
before player planting; treating every new planting as obstruction made preferred
water permanently displace the dam. Typed per-structure planting compatibility now
preserves supportive water/reeds at dams while grove and terrain changes still
interrupt them. The production world test covers attraction, arrival, active work,
displacement, undo and rebuilding the same object. No test-only traversal substitute.
Foot support now agrees across movement, regrounding and snapshot restore; historical
center-sample snapshots remain readable. Streaming CPU routes pass (`175652-run-d4c97`).
All eleven CPU biome scenarios also pass (`180818-run-892b6`).

The native host now prevents duplicate end-editing/Send submission and ignores blank
submissions. The field journal uses a reusable scrollable native document sheet,
with game-owned known-place/individual entries. Rare habitat activity is shown only
for a currently visible animal, so it is not a remote tracker. Both native changes
still require post-build computer-use checks. Earlier baseline native keyboard taps
moved the player; native Send falsely showed an error after an accepted greeting.
Earlier native construction menu successfully placed a bench; its ground contact
still needs visual refinement.

Soundstage built, and the renderer numerical check passed on Apple M4:
`.build/sanctuary-native-20260912/fields-renderer-check/report.json`.
Sky traversal verification passed 131072 candidate rays, 28533 certified segments,
256797 density samples, zero violations/skipped density. Invalid vegetation height
rejected atomically. `flora-native-checks.json` retains these results. Native branch
Droop edit changed the source/bounds and native Undo restored it.

First actual vegetation review is **not accepted**:
`flora-first-review.json` and the three `biome-*-first-study.json` files in the native
root. Root viewed quarter images; Astra independently reviewed all six quarter/above
images. Broadleaf is visibly bare with flat fern-like fans; reject. Willow is legible
but has regular sparse curtains and an exposed radial scaffold. Palm is recognizable
but needs trunk/frond refinement. The new generators are standalone Soundstage
subjects and are not yet scene vegetation. Astra's next proposed iteration targets
rounded layered broadleaf foliage, varied willow curtains and modest palm refinement.
Shared dark-side readability remains poor; asset exposure was not changed.

Native Style board was exercised; report:
`.build/sanctuary-native-20260912/field-flora-stage/workspace/.soundstage/studies/style-board-20260912-180930-b141f8/index.html`.
Root read its HTML source and actual broadleaf noon/sunset and seed sunset PNGs;
interactive HTML viewing remains unverified because earlier browser local-file
navigation was rejected. No baseline accepted. Soundstage has been closed cleanly.

Current running work: root native streaming render harness
`.build/test-runs/20260912-181115-render-8e543` (exec session44663), serialized GPU.
All production source owners remain frozen. World_content is doing bounded density
research/CPU experiment only. Next: inspect native Alpine/water boundaries, then
verify Send/journal/save and measure full live cache publication and stale requests.
The cache candidate adds staging resources and takes multiple seconds to publish;
direct sunlight/shadows can currently lead cached illumination. This remains a
visual coherence limitation, not a final sky implementation. User's raised engine/
world-class ambition is recorded in the specification and backlog; game incomplete.

### Native streaming visual rejection and active follow-up

`181115-render-8e543` completed both native streaming state checks, but actual PNGs
are rejected: Alpine before is flat olive and after nearly black across the frame.
This candidate has not repaired camera visibility. Creek's near water edge is now
rounded, but a large reflective sheet with stretched dark bands remains. Astra is
probing actual terrain/mount/water geometry; do not assume the prior far-grid cause
explains this new view. Evidence: the report's `native-0/runtime/captures` Alpine
before/after and `native-1/runtime/captures/creek-bank-after-2227c8fb.png`.

The density research and CPU experiment are retained in
`studies/SANCTUARY_BIOME_DENSITY_RESEARCH.md` and
`.build/sanctuary-native-20260912/biome-density-experiment`. Stable split-query IDs
passed; naive Python enumeration costs 98–186 ms p95 per chunk and cannot be put
on the native frame path. Candidate densities are not visually accepted.

Current owners: Astra creature_art owns BiomeWorldPresentation/World and confirmed
water fixes in LivingWorldPresentation; Astra biome_flora owns the second revision
of BiomeVegetationDesign and its study report. World_content owns new habitat-field/
vegetation-record source and tests, with no scene geometry edits. Root owns native
GPU/UI/cache measurement and integration. All other owners are frozen. Root's owned
Sanctuary app uses `cache-ui-check/runtime`, lifecycle session14877. The binary's
source digest is b45c057e744c3575282d363bb39e706ea74ba424020a6f5a647f0048345a9081;
concurrent source edits are not part of that compiled native experiment. Close this
owned app before rebuilding. No baseline accepted; default player saves untouched.

### Live cache publication and native controls

Owned `cache-ui-check` app is now closed (session14877 completed); no app remains.
`cache-publication-native.json` / `run-cache-publication.py` retain two18s live
windows on native1920×1080 AppleM4,4xMSAA, modeldisabled. Binary source digest
b45c057e744c3575282d363bb39e706ea74ba424020a6f5a647f0048345a9081;
shader57cbd3c566e8427ec9f397adbf4a6b2a2df1ebdbafdc7546869502b1dc79b17b.
Automatic morning→noon GPUmedian13.224,p9514.206,max16.531ms; presentedmax16.667ms.
Generation2 published after~5.95s construction. This one window removes the earlier
200ms boundary stall, but is not full performance acceptance.

Rapid golden/sunset/noon requests: GPUmax25.892ms and presentedmax83.333ms fail
specified limits. Outlier first appears at golden request, drawablewaitmax58.938ms;
GPUmax occurred in waiting phase, actual densityframesmax15.069ms. Profiling was off,
so underlying scheduling/OS cause is unresolved. Only latestgeneration5 published;
generations3/4 retired, active snapshot held until completion (~10.22s). NoGPUerrors.
Added cache allocation318988288bytes (~304MiB), total1.3235GB. Directsun/shadows lag
coherence fix and an explicit published/exact numerical diagnostic are active.
Root inspected before/after PNGs; same cabin scene is legible, but dark foliage
undersides and simplified silhouettes remain. These are different times/lighting,
not a matched image equality test. No baselines accepted.

`native-input-journal-check.json` verifies actual Return increments population
revision exactlyonce; blankSend followed by validSend adds exactlyone more and
accepted greeting. No duplicate rejection message. Native journal picker/Apply
showed readable visitedCabinGlade and observedSunhare in its sheet; Close restored
gamefocus. Creature response stays nonverbal by specification. This check used an
isolated slot, not the default player save.

Additional active owners: wildlife now only AnimalRelationship.swift and its tests
for the concrete four-second full-trust/spam exploit; game_scenarios owns new
ConnectedJourneyScenarios.swift to exercise one actual connected route. Presence
and replay-fixture assumptions must be reconciled before integrated validation.

### Grade, coherence, pacing and the packed-grass shadow defect

Soundstage second vegetation build passed27.92s; Sanctuary grade/cache build passed
25.94s. `flora-second-review.json` retains15 real captures. Root inspected three
quarters, broadleaf above/back/indoor and workingtree noon/softbox style-board frames.
Willow and palm are ready for provisional scene-context evaluation; broadleaf remains
held for stacked smooth foliage masses. No asset/baseline published. Native Play and
Styleboard were exercised. Secondboard: `flora-coherent-stage/.../.soundstage/studies/
style-board-20260912-183334-927ed2/index.html`; HTML source read, interactive viewer
still unverified. Soundstage closed cleanly.

Fullquick354tests ran (`183546-quick-4a64b`), with3failures in retired rescue fixtures
that assumed old trust progression/static animal gaze. New grade/density/pacing/cache/
save-height tests passed. Follow-up source fixes preserve the real visibility gate;
legacy request transport now targets its matching legacy relationship. Traversal
fixtures explicitly stage established relationships and still use production controls.
Legacy refusal/familiarity and carry/replay are separated. These test fixes need rerun.

Pacing now caps calm presence at.10 familiarity over12active seconds, preserves higher
saved trust, and prevents trust rewards within300simulation ticks of the last request.
Responses still occur and persist immediately, including refusals. Accepted shared
interactions earn further trust; no offline decay or new required save fields. This
removes instant fulltrust, but native relationship pacing/enjoyment remains unproven.

Terrain source probe (`mesh-diagnosis/`) proved Alpine's blank view came from a~72°
near-source ramp, not remaining far overlap. Bounded20m route correction preserves
summits811.710/584.195/555.742m and reduces sampled approach longitudinal grade to
.52945m/m (~27.9°), fullcrossslope.73207 (~36.2°). Tests passed. Slot opening now
reconciles obsolete source heights with real support while preserving valid poses;
strict same-source simulation restore remains separate. Diskreground/replay tests pass.
Native `alpine-grade-native.json` shows sky/valley again but severe diagonal shadow
bands. Its original mounted fixture XZ/history is unchanged; only initialY is recomputed
from revised source. Actual production9m ride crossed the chunk boundary. First whole
controller-checkpoint reopen comparison was false; detailed repeat is retained and
identical (diskload resets the transient autosave timer). Do not claim whole transient
controller-memory equivalence for the first attempt.

Final profiled native cache artifact `coherent-game-cache.json` uses source
`dd86fe7c801cc3b5e9176afb703489bf6cf989198795084cc31263e4bb89b535`.
Two18s windows: automaticGPUp9514.292/max19.496ms, rapidGPUp9514.626/max26.025ms;
both presentedmax33.333ms. All106samples have coherent actualsceneSun, including52
pending changed-sun samples. Latestgenerationonly publishes. Strict livecache/coldexact
comparison passes33554432sky +8192irradiance components atsample19.899868/weatherTick9;
explicit diagnostic293.885ms is outside live windows. RapidGPUmax still fails25ms target;
earlier83.333ms outlier retained. See skyresearch report for counter/latency limits.

Shadow A/B `shadow-isolation.json` confirms diagonal bands disappear when only shadow
receiving is disabled. Surface.metal restored byte-for-byte and pipelines reloaded.
Read-only diagnosis found the concrete cause: regional packed GrassBlade32byte buffers
enter the ordinary shadow vertex function expecting64byte Vertex records and draw
9vertices perblade. This is an invalid buffer interpretation. Astra sky_cache now owns
Surface.metal/MetalRenderer.swift repair with dedicated grass-shadow reconstruction
sharing the production blade function and atomic pipeline replacement. No game-name
shadow exclusion. Native Alpine/Creek/cabin and numerical renderer checks are next.

Current app: root-owned Sanctuary `coherent-game-check/runtime`, lifecycle session81387.
Close before rebuilding. Active source: astra_biome_flora newNatureMagicPresentation,
LivingWorldPresentation, subjectregistration/report; rootExperience age/water/telemetry
hookup. Nature candidate uses authoritative saved eventtime,1sgrowth and bounded8rings;
not built/native-reviewed yet. Skyworker owns packedgrassshadow repair. Other workers
frozen. Fullgame incomplete, player defaults untouched, no baselines accepted.

### Native review checkpoint — grass shadows / saved-time nature, 18:55

Closed the owned coherent-game-check app (session81387); no native renderer is
running. Latest quick `20260912-185038-quick-42098` ran354unit tests with one
failure: the explicit familiar-flier fixture left its landmark before a discovery
tick. Added one production tick before flight. Focused GameContractTests then
passed both tests (27.423s); this fixes the only remaining unit failure without
repeating every passing unit. Boundaries and production smoke remain next.

Building Soundstage then Sanctuary serially with the dedicated procedural-grass
shadow pipeline and saved-time nature growth/ripple source. These changes still
need numerical shader checks and native image/motion review. No baseline accepted.

User added reactive grass and mud footprints. Recorded in the game specification.
Astra's plan is a separate bounded32m/128×128 local influence grid, typed actual
movement events, shared scene/shadow bending and saved-age recovery. It must not
reuse the coarse periodic wind grid. World content is planning bounded persisted
contact history and movement hooks; sky_cache owns shared-field/shader design.
Both are read-only during this build freeze. Footprint art must also be Astra.

### Ground response integration / repaired native shadows, 19:12

Astra grass shadow repair is now natively verified: large diagonal bands gone in
both Alpine and Creek. Actual images inspected in `alpine-shadow-repaired-native.json`
and `creek-shadow-nature-review.json`; screenshots retain binary source/shader hashes.
Alpine slopes/long views remain sparse and visually weak, not accepted final art.
Nature Soundstage strips and embedded HTML viewer inspected through native controls.
Water200ms disk state/seed/phase exact; image differs at most1/255 over10×2distant
pixels. Native undo×2 and empty-undo rejection passed exact snapshot preservation.
See nature study report for evidence and remaining look/tooling issues. Both owned
apps closed; no GPU/native session currently running.

Ground influence source is integrated but not yet built/tested: FieldCore typed
Double-time contacts and128²/32m bounded raster, triple1.5MiB GPU buffers, shared
scene/mesh/shadow root-bending helper, strict1.5m culling expansion. One support
height per cell selects strongest conflicting layer instead of bending another
floor. Sanctuary saves≤256events; actual accepted walking/riding/wildlife sweeps,
immutable alternating footsteps every0.65m, saved cadence, no airborne/teleport
trails. Root wired GameExperience input and engine/stage diagnostics. Soft-soil
query rejects water/buildings/stale support; project footprint source uses cached
nine-sample support and bounded64impressions. Full combined checks/native review
are next; no physical-response performance or appearance claim yet.

Willow/palm scene substitution source is frozen with shared trunk collision and
uniform wood/foliage transform at existing IDs. New conifer Soundstage candidate
also frozen; no conifer scene integration before native review. Root owns integration
and soil query/tests; world_content frozen contact save/movement layer; sky_cache
freezing engine influence; astra_biome_flora finalizing footprint source/registration;
astra_creature_art frozen botanical source. Next: combined quick, connected journey,
serialized builds, Soundstage footprints/conifer, native grass/footprints recovery,
reopen/rejection and live1080p cost including cloud/wind updates.

### Constraint-grown biome research steering, 19:22

User asks to grow terrain/biomes from environmental constraints (weather, soils,
water; tectonic uplift as an illustrative mechanism). Astra is running a bounded
primary-source research + CPU prototype in Tools/Experiments/sanctuary_biomes,
with no production terrain replacement yet. Compare ridge/drainage/moisture/soil
coherence with existing radial biome assignment; retain seeded metrics, source,
replay commands and research maps labeled separately from native render evidence.
Integration must preserve landmarks, authored edits and player saves.

Combined ground-response quick PASSED:374unit tests, boundaries and production
smoke, artifact `20260912-191856-quick-f49bc` (77.431s unit portion). Starting the
connected Cabin→Meadow→Creek production journey, then serialized native builds.

### Constraint experiment and first ground response review

The three-seed constraint biome experiment is complete; see
`studies/SANCTUARY_CONSTRAINT_BIOMES_RESEARCH.md`. Drainage reaches outlets and
wind reversal moves moisture on fixed terrain, with exact separate-process replay
in the recorded Python environment. HOLD production adoption: angular D8 channels,
soil striping, dry coverage bias and unresolved lake supply policy. Final scripts
are being retained in `Tools/Experiments/sanctuary_biomes`. No player terrain moved.

Connected journey artifact `20260912-192228-run-035d9` failed at a real Brookweaver
dam on the creek centreline, not a streaming failure. The scenario now takes an
actual bank detour and shallow crossing; no collision bypass. Detour rerun pending.

Ground renderer build exposed Metal's reserved `vertex` identifier; renamed the
local to `sourceVertex`. Fixed native renderer check passes on M4, retained under
`ground-response-renderer-check-fixed/report.json`; initial failure retained too.
The 374-test quick predates the new shared grass authoring hook and route detour.

First Soundstage review inspected actual boot/conifer PNGs. Both HOLD: boot looks
like a raised slab and remains visible at recover24s (studio grounding cancels
burial); conifer reads as flat leaves and spaced naked tiers. Astra flora owns
the bounded trace fix, Astra creature art the conifer-only revision. Sky worker's
new `grass-contact` subject and optional AnimationDefinition surface influence
hook are source-ready but unbuilt; exact study/support-height review is next.

Root closed its Soundstage lifecycle session9344. No owned native app or GPU
work remains running at this checkpoint. World content now owns a new pure
environment query + tests only, separating habitat climate from surface wetness;
no production terrain or save changes. Root owns integration/next serialized
checks and native walking, traces, willow/palm context and live performance.
No visual baseline accepted; full game remains incomplete.

### Environmental source / connected route checks

New pure `SanctuaryEnvironmentField` separates stable habitat moisture/temperature
from live rain wetness; bounded upwind terrain probes create windward/lee response.
This is an authored climate/soil proxy, not geological or meteorological simulation.
It is not yet wired into production placement/soil eligibility; full query costs
11 height calls, so decompose/cache static climate before footprint integration.

Shared quick `20260912-194315-quick-aad57`:380 tests, one incorrect new temperature
ranking expectation. Corrected test isolates same-coordinate elevation lapse;
focused six environment tests then pass (`/tmp/sanctuary-environment-focused.log`).
Other379 passed; module boundaries pass. Connected bank-detour journey now PASSES
in `20260912-194653-run-4277c`; native route review still pending.

Native ordinary10W inputs moved1.2m, generated5bounded contacts with15activecells,
no contact errors, and no mud prints on dry cabin soil. Named-slot saved expedition
and camera reopened exactly; only the transient secondsSinceSave resets to0.
Evidence `ground-native-first.json`; real PNG inspected, grass remains sparse and
its visual contact response is not yet accepted. App21415 closed normally.

Footprint/conifer revised source and grass-contact studio hook frozen. Root is
building Soundstage then Sanctuary serially. Researcher runs one bounded irregular
planar drainage study under .build only; no production edits. All otherworkers
frozen. No baselines approved and no full-game completion claim.

### Native ground response, environmental research and live model checkpoint — 20:19

Full game incomplete; no baseline approved. Root closed owned Sanctuary session6923;
no native/GPU session remains running. Player default saves untouched.

Revised footprints and shared grass contact field were inspected in Soundstage and
the native game. Grass study saved/reloaded pixels exactly; 1m wrong-support study
matches settled exactly, while contact sweep changes150270pixels. Native Motion
restart/frame/play/pause/strip and embedded HTML review exercised. Grass bends
locally with grounded roots, but blades remain angular and scene coverage sparse.
Boot/soft traces now have <2mm broken rims, zero bind minimum and exact collapsed
recovery tails; the former raised slab/24s residual is fixed in actual PNGs.
Artifacts: `grass-contact-first-review.json`, `trace-conifer-second-review.json`
under `.build/sanctuary-native-20260912/`; provisional appearance only.

Actual creek native keyboard movement progressed from dry bank (0prints) to soft
bank (11prints). Named-slot expedition state reopened exactly. Water reduced11→3,
deck11→5, raised support11→0; restoring support restored eligible traces. At390.55s
all prints were recovered. Actual soft-bank/water-cover images inspected; deck,
raised and final recovery captures retained but not yet individually viewed.
`ground-creek-first.json` retains fixtures, actions, state and captures. Footprints
are functional overlays, not displaced terrain holes. Bare green substrate and
hard circular edited-water boundary remain visible defects.

Short scripts/profile 8s live1920×1080 AppleM4 creek checks include wind, cloud and
normal cache publications. StationaryGPU median/p95/max12.154/14.272/16.556ms;
walking12.239/14.084/15.963ms; presented max16.667ms each. CPU simulation max10.47/
10.30ms retained. Contact-gridCPU p95 .123/.114ms, max .166/.177ms. Triplebuffer
1,572,864bytes. Walking exercised29real collision-resolved move commands. This is
sparse-creek evidence, not whole-game performance nor isolated on/off contact cost.
Reports `ground-creek-live-profile.json` and runtime/profiles retain provenance.

Denser cabin live/model8s pair: liveGPU p95/max14.775/18.968ms; model15.065/16.775ms.
Model run presented max33.333ms, CPU simulation max12.755ms. A209msGPU/193msframe
spike occurred AFTER the measured live window when profile restored paused state;
retain separately as pause-transition defect, not a normal-cache live measurement.
No GPUerrors. Model wait proposal completed1609ms, validated target sunhare-001,
and persisted one decision. Interpreter remains opt-in/defaultOFF because prior
ambiguous input failed. Replay script `run-ground-model-profile.py`, initial/terminal
snapshots and requests `ground-model-live-profile.json`, exact profiles in
`ground-model-live/runtime/profiles/`. Text classification does not establish animal
intelligence. Native hello screenshot reveals Sunhare facing away; shared greeting/
wait brain early-return diagnosis is being fixed by Astra, then reviewed natively.

All above native studies use embedded source415e26d7adaa510cf1569bbdb5cd4b55b27e0a49c9c12602bd1b140cd259dd1b,
shadera90796930a00c944d56548771b6d3737403af7db978f4af843460814fb8c3a8d.
They predate regional material14/conifer scene/environment split changes.

True Delaunay drainage research supports topology change: alignment100→22.27%,
eighth circular moment1→.0165, exact replay/outlet/area invariants pass. One seed
only; continuous banks/lake occupancy/route/save contracts remain unresolved. Read
`studies/SANCTUARY_CONSTRAINT_BIOMES_RESEARCH.md`; production adoption remains HOLD.
Environmental surface path now avoids all upwind/height probes when supplied support
and grade. Focused environment+botanical10tests pass, log
`/tmp/sanctuary-environment-botanical-integration.log`. Conifer scene integration and
material14 source are frozen but not natively reviewed. Ground study subjects:
`ground-creek`, `ground-alpine`, `ground-meadow`; source report
`studies/SANCTUARY_GROUND_APPEARANCE_SOURCE.md`.

Current owners: root integration/native/builds/evidence; world_content exclusively
GroundSurface.swift+GroundSurfaceTests for environmental substrate integration;
astra_biome_flora exclusively CreatureSimulation.swift+new greeting-facing tests;
astra_creature_art terrain recipe plan docs only. Others frozen. Root next focused
checks/builds, material14 Soundstage views/styleboard/wet-dry, conifer regional
context, greeting direction, connected native route, live regional material cost.
