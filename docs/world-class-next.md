# The world-class engine: the next push

*An audit and a list of levers, written 2026-10-25 after B13 landed on `engine/world-class`, for
Ryan and for the agent that writes the next plan. It is not a plan: nothing here has a gate yet.
It checks a list of ideas another agent proposed against the code and the bench, adds what five
read-throughs of the engine found, and says which of the industry's 2025–2026 techniques fit a
field engine on an integrated GPU, and which only this engine can do because its compiler sees
the whole game. Every number is from `docs/bench.md`, a saved build, or a line of code that is
named; a number no run has measured is marked **estimated**, and an idea no test has tried is a
**hypothesis**. Line numbers are as of `024d6f77` with the review fixes that were uncommitted in
the checkout that day, so a few may have moved. Sources are at the end.*

## 0. The short version

- **Where it stands.** On the M4 Air in Chrome at 1080p (`docs/bench.md`, B13): the clearing holds
  60 fps (GPU frame 10.2 ms); the Last Green's opening misses by 2 ms (18.9 ms) and its walk by
  its hitches (12.6 ms median, 9 of 1,200 frames over 33 ms); the Last Green is playable 10 to
  14 s after a cold open against a 5 s budget; its GPU memory peaks at 781 MiB; nothing has been
  measured on any other device, and no competitor scene exists to compare with.
- **The pasted list is right about what to aim at and mostly wrong about why.** The terrain
  doesn't cook every frame; the shadow maps are already cached; the cloud dome is already
  amortized; the probes' 2.7 ms is the first bake, not a steady cost; "154 pipelines made
  lazily" names a mechanism that doesn't exist; the memory figure is stale. §2 has each claim,
  its verdict and the corrected lever.
- **The biggest single finding:** a 256 m tile's arrival re-cooks every terrain level whose
  square overlaps it, height and normal included, though the tile's fields only change the
  cover; at least seven whole levels of 16,641 texels each, each texel five height evaluations
  plus the cover. That is the opening's 4.65 ms "terrain cook", the near cascade's 2.45 ms (a
  tile arriving marks the whole near map stale) and the prime suspect for the walk's hitches
  (§2.2, §3.2).
- **The cold start is a chain, not a pile:** 7.4 s of concurrent pipeline builds (20 s if
  serialized), then plant kinds two a frame, then the wood, then prototypes, then built things one
  a frame, then casters, then an 11-strip probe bake: playable at frame 220 while the floor's own
  "ready" print is at frame 41. 2.4 MB of the 5.95 MB of WGSL is the realizer's kernels
  instantiated once per field type (11 copies of `place_vertices`); the WASM is 52 % data
  section; it is compiled from bytes, not streamed, and served without compression (§3.1).
- **What only this engine can do** (§5): fold `Quality` and other constants into shaders;
  realize through field *tapes* so one pipeline serves every creature at load and the per-kind
  kernels arrive later; infer where `f16` is safe from the compiler's own interval derivation;
  prune fields per block before evaluating them; derive crack-free continuous LOD by projecting a
  coarse mesh onto the field; classify draws static or moving by data flow; and train tiny
  networks at build time as constants, because the build already runs the program.
- **The order I'd work in** (§8): tooling that can attribute a hitch; the cold start; the
  re-cook and the hitch sources; the device matrix and competitor scenes; then the look
  (page-cached shadows, probe visibility, volumetrics, rock and water); then the frontier.

## 1. Where the engine stands

### 1.1 The numbers that matter

From `docs/bench.md` (B13, `78a31b6c`, toolchain 0.6.3, M4 Air, headless Chrome 155, 1080p):

| Scene | GPU frame p50 / p99 ms | Frames over 33 ms | Playable, cold | Pipelines' build | GPU peak |
|---|---|---|---|---|---|
| The clearing's path | 10.2 / 11.2 | 0 of 600 | 4.0 s | 3.1 s | 513 MiB |
| The Last Green's opening | 18.9 / 25.7 | 8 of 480 | 14.1 s (10.1 in later runs) | 7.5 s (4.3–4.5) | 781 MiB |
| The Last Green's walk | 12.6 / 18.5 | 9 of 1,200 | 13.9 s | 7.4 s | 781 MiB |
| The capybara's run | 1.1 / 2.0 | 0 | 0.2 s | 0.6 s | 82 MiB |
| The herd | 4.7 / 6.7 | 0 | 0.2 s | 0.3 s | 112 MiB |

The opening's GPU labels (`wrela profile --chrome`, `docs/bench.md`): terrain cook 4.65 ms,
vegetation 3.93, probes bake 2.69, near cascade strips 2.45, clouds 2.35. These overlap on the GPU
unless `--serial` is passed, so they are each label's own span, not shares of the frame.

The paced 60 Hz numbers understate the GPU's margin: the M4 lowers its GPU clock until a frame
fills about three quarters of its interval (vision, "Budgets"), so a scene needs room well under
16.7 ms, not just under it.

### 1.2 What a build is

The Last Green's bench build (`games/last-green/build/`, `wrela pipelines games/last-green --json`):

| Thing | Count or size | Note |
|---|---|---|
| Pipelines | 169 (115 compute, 54 render); 170 by `wrela pipelines`; 152 in the release build | 47 reachable only under a runtime condition (`conditional` every site) |
| WGSL | 5.95 MB over 169 files; largest 238 KB (`cook_materials`); median 11 KB | the shipped budget is 256 KiB per pipeline |
| WGSL by module | `realize` 2.42 MB in 27 pipelines; `character` 0.92 MB in 14; `wood` 0.62 MB in 21; `terrain` 0.56 MB in 8 | `place_vertices` ×11 (1.1 MB), `cull_blocks` ×11 (0.97 MB), `cook_colours` ×8, `cook_level` ×4, `cook_materials` ×3 |
| Pipelines needing a device feature | 0 | no `f16`, no subgroup path is compiled into the game |
| WASM | 8.37 MB: 3.81 MB code, 4.36 MB data (38.8 % zero bytes, one passive segment) | gzip −9: 8.17 → 2.95 MB; WGSL 5.79 → 1.27 MB |
| Bytes a cold open loads | 15.7 MB uncompressed: 92 % is WASM and WGSL; tiles 246 KB; `far.bin` 103 KB | |
| Compile, serialized | 20.2 s: `skin_vertices` 1.62 s, `cook_materials` 1.26, `place_vertices` 1.13, `cull_blocks` 0.68, `cook_level` 0.58 | the same shaders vary 2× run to run |

Each `realize` kernel is instantiated per field type with the field's code inlined (the compiler
monomorphizes on types, `spec/language.md` §18.9; one WGSL module per pipeline), so every creature
or built-thing kind costs a copy of the realizer's whole kernel set.

### 1.3 What the stills show

The B13 lookbook stills (`build/stills/b13/head/`) against the B1 set: the meadows, the beech
high forest, the great tree's crown and the tower's masonry read well; these are the engine's
strengths, and B8's occlusion, bloom and exposure changed them little. The gaps that are the
engine's, not the content's:

1. **Rock and cliff.** The crag's top (`04-the-crags-top`) shows the heightfield's terracing along
   its edge and a smooth grey sheet for its face; the waterfall's wall (`09-the-waterfall`) is the
   same sheet. The terrain is a heightfield plus solids (`Terrain::substrate_height`,
   `engine/terrain.wrela:47-54`), and the solids have no near-camera detail.
2. **Falling and fast water.** The waterfall is a blown-out white quad; no spray, foam column,
   mist layering or wet rock. The frame has particles (`engine/particles.wrela`, 262,144 budget,
   sorted, lit) and the Last Green emits none (no `emit` override in `stage.wrela`).
3. **The far land.** Low-poly snow mountains, flat-shaded; far trunks are bare grey poles under
   crowns (`13-the-towns-site`); M6 called the same thing "dotted far land".
4. **Cloth and dark materials.** The climber reads as a flat black cutout (`16-the-fawn-in-the-
   glade`): no sheen, rim or fold shading; the hem's "licks" are purple-edged spikes. The cloth is
   PBD points drawn by the climber's own shader (`content/climber/draw.wrela`).
5. **Volume.** No light in the air: the shafts exist (`engine/shafts.wrela`) but are off in the
   floor's view by default (`stage.wrela:174, 616`); there is no local fog, no scattering under
   the canopy, and the sky's aerial perspective is the only depth cue.
6. **Materials under the eye.** Bark tiles visibly at a metre; grass blades near the camera are
   large and faceted; leaves have a wrap-lit transmission term but no backlit glow against the sun.

## 2. The pasted list, checked

Each claim, what the code says, and the lever that survives.

### 2.1 Cold start

| Claim | Verdict | What the code and the runs say |
|---|---|---|
| "5.3 s of 11.6 is building 154 pipelines" | Holds in shape, not in number | 169 pipelines in the bench build; 7.4 s concurrent (`pipelines.json`), 20 s serialized; the bench defeats every cache on purpose (`salt=3`, `studio/bench/main.wrela:257`), so this is a first-ever visit. Frame 1 waits for the last pipeline (`runtime/browser/src/worker.ts:727`). |
| "build only the first-frame pipelines before showing anything" | Holds; nothing like it exists | No lazy path, no first-frame set: all 169 start at once, biggest first (`gpu.ts:356-432`), and the first command that needs an unbuilt pipeline stalls every command after it (`gpu.ts:816-818, 1043-1060`). The toolchain gap "a branch under a false constant still makes its pipelines" (`docs/toolchain-gaps.md`) is the same problem from the compiler's side. |
| "ship a pipeline-cache warmup" | Unmeasured | The only cache is in-memory, for hot reload (`gpu.ts:343-350`). Whether Dawn's own disk cache serves a second visit is unknown; nobody has benched a warm open. |
| "prioritise the tiles under the eye" | Already so, near enough | Tiles within 350 m, four in flight, placed one a frame (`stage.wrela:779-794, 1986-1994`); built things are realized nearest first. Tiles are 246 KB of a 15.7 MB load; they aren't the cold start. |
| "the 8 MB WASM is worth a look" | Holds, for different reasons | 52 % is a data section of build-time constants (`BAKED`, `OBSTACLES`, `COURSES`, `VIEWS_GRID`, `TOUR` in `games/last-green/history.wrela`), 39 % of it zero bytes; it is compiled with `WebAssembly.compile` from bytes (`program.ts:472`), not `compileStreaming`, so V8's code cache can't key it; and no server sets `Content-Encoding` (gzip would take it from 8.2 to 3.0 MB). |

What the list missed: the load is a **serial chain** after the pipelines (`stage.wrela:2009-
2081`): 26 plant kinds at two a frame (done at frame 12), then `Wood::new`, then three bank
prototypes one a frame, then every built thing one a frame (each a whole `realize_rigid` with a
readback, `stone.wrela:323-336`), then "casters complete", then the probes' first bake in 11
strips; "playable" prints at frame 220 (13.9 s) while `main.wrela`'s "floor ready" prints at
frame 41. Most of those links don't depend on each other.

### 2.2 The Last Green's GPU frame

| Claim | Verdict | What the code says |
|---|---|---|
| "terrain cook 4.65 ms every frame; it shouldn't run when the eye isn't moving" | **Wrong mechanism** | `Clipmap::follow` cooks a level only when the eye moves it two cells (then 258 texels, not 16,641) or when the level is marked uncooked (`engine/terrain.wrela:440-447, 782-805`). What marks levels uncooked is `write_fields` from a tile or a field refresh (`:364-407`), which clears **every level whose square overlaps the tile**: a 256 m tile overlaps levels 5–11 at least, so one tile re-cooks ≥7 whole levels, ≈116k texels, each five `height` evaluations plus the cover, although fields only enter `cover_with` (`:48-52`), never the height. The opening jumps, then streams tiles and refreshes neighbours' fields (`stage.wrela:1766-1790`): that is the 4.65 ms. |
| "cached shadow maps that redraw only around moving casters would take most of the cascade cost" | **Already built**, misdescribed | The two cascades are static maps replaced a strip a frame (near 2048² over 144 m in 12 strips, far 2048² over 2.4 km in 48; `engine/shadow.wrela:251-254, 371-405`); moving casters have their own 1024² map (`frame.wrela:695-696`). The near strips in the opening are whole-map replacements that **tile streaming** triggers: any tile arriving or leaving marks the near cascade stale (`stage.wrela:1999-2004`). The lever is partial invalidation of the cached map by the tile's footprint, not caster-split caching. |
| "the cloud dome could march at a quarter res and reproject" | **Partly** | The dome (4096×1280) is already marched 1/32 a frame, 163,840 rays, and blended 0.75 into the last round (`engine/sky.wrela:55-65, 825`); it is indexed by direction, so eye reprojection doesn't apply. The per-ray cost is 8 light samples per dense sample (`:505-511`); a cheaper light march (a transmittance LUT per weather cell, or 2–3 samples with Nubis-style multi-scatter approximation) and a 2048×640 march upsampled to the dome are the levers. |
| "vegetation 3.9" | Holds; the label is wide | "vegetation" is `scene.shade`: grass (three tiers plus flowers), built things, bank stones, the wood's cards, limbs, impostors and volumes, and rocks (`stage.wrela:462-548`). Grass places its blades on the GPU **every frame**, five dispatches with per-cell noise, up to ~2.2 M blade vertices, with no depth-pyramid test and only the flowers in the depth prepass (`engine/grass.wrela:204-402, 1770-1778`). |
| "probe bake 2.7" | Holds for the first bake only | The label is the first bake: 11 strips of 640 columns × 8 layers × 32 rays, re-run whole whenever the eye moves 30 m or occluders change (`engine/probes.wrela:862, 1023; frame.wrela:1875-1890`). The steady state is "probes again": 32 columns × 8 × 64 rays a frame, a full round every 205 frames. In the opening both run. |

### 2.3 Hitches

| Claim | Verdict | What the code says |
|---|---|---|
| "pipelines made lazily when first reached (B9 added 13 'reached but made on use')" | **Not supported** | No such path exists in the runtime or the docs; B9 is particles and transparency. Pipelines block the frame only during the cold start. |
| "realizer meshing spikes" | Partly | The characters' realizer is budgeted and sliced (`engine/realize.wrela:827-845, 1400-1407`). Built things are realized **whole**, one a frame, at a cost the planner only estimates (`e(4000)`, `cook.wrela:114-117`), with a readback each. Load-time, unmeasured. |
| "tile streaming landing in one frame" | Partly | Placement is one tile a frame with a 1 s fade. But (a) **every finished tile job's field write lands in the frame it finishes** (`stage.wrela:1733-1800`) and each write re-cooks ≥7 terrain levels (§2.2); (b) the far layer's 65 tiles are placed and loaded into the wood in one frame with no planner gate (`:1897-1957`); (c) the arrival cover sets visibility to 1.0 at once. |

What the list missed: the planner's own whole-frame steps: the sky's noise and weather ("a
frame that cooked the air and the clouds together took 137 ms", `frame.wrela:1721`), a far
cascade strip at 11 ms in Chrome (`cook.wrela:112-115`), a plant kind at 40 ms, and the rule that
the first optional step of a frame runs whatever its size (`cook.wrela:17-19`). And the fact that
**no saved run can say what the walk's nine hitches cooked**: the bench overwrites `timings.json`
and the paced `frames.json`, and every frame's `phases` is empty because the game never calls
`std::time::phase`. The first step on hitches is a measurement, not a fix (§3.5).

### 2.4 Memory

"1.2 GB" is stale: 781 MiB after B13's fitted meshes and compressed impostors (1,468 before). It
is still too much for an 8 GB laptop whose GPU shares that memory, and the number is the whole
program's peak, not a breakdown. The budget module exists (`engine/memory.wrela`, 1,024 MiB
**proposed**) but its realizer branch can never fire: no caller names a realizer share. The
cluster path, if it ever ships, holds 667 MiB of lists at the banks (`stage.wrela:63-69`).

### 2.5 Testing the claim

Holds entirely. One device, one browser, no competitor. §4.6 says what the matrix should be.

### 2.6 The crazy stuff

| Idea | Verdict | Why |
|---|---|---|
| Whole-scene virtual geometry | **Hold; narrow it** | Accurate that only `examples/clusters` draws through `engine::virtual`. But the measurement that turned the banks' switch off stands: clusters cost 20.08 ms against 19.09 forward in Chrome and 667 MiB of lists (`stage.wrela:63-69`); the path has no group DAG, no skinning and no streaming (every level resident, `virtual.wrela:229-389`); the trees are cards and will stay cards (foliage is not the cluster case even for Epic). Months, for a win the numbers don't yet predict. The field-native version is cheaper and better (§5.6). |
| Virtual shadow maps | **Yes** | The cascades are already cached and strip-amortized; the missing half is page-granular invalidation. §6.1. |
| Real GI | **Yes, in steps** | The probes store L1 SH with **no visibility term** (`probes.wrela:37-49, 727-781`), so light leaks through walls and trunks; interiors are "unsolved" in the vision for this reason. A visibility term first, then a cascade or surfel layer. §6.2. |
| Volumetrics | **Yes** | Nothing local exists: aerial perspective from the LUT, a marched mist for the falls, shafts off. §6.3. |
| Physics and animation on the GPU | Partly | Cloth exists (PBD, CPU, presentation-only, `content/climber/cape.wrela`); grass already bends round a cloak capsule and deletes blades under feet (`grass.wrela:1392-1431, 248-255`). FFT water is the wrong tool for a mere and streams; flow and wave particles are right. §6.5. |
| Rollback netcode | Game work | `Timeline::correct` has no caller outside tests; there is no input delay, prediction, snapshot transfer or peer identity beyond a 64-bit token. The sim's determinism is the hard part and it is done. |
| A frame debugger in the browser | **Yes, first** | §3.5. |

## 3. The levers the surveys found

Grouped by what they move. Each names the evidence and a sketch of the fix; wins are
**estimated** unless a run is cited.

### 3.1 Cold start (10–14 s → 5 s)

1. **Compress and stream.** Serve WASM and WGSL compressed (gzip −9 is 8.2 → 3.0 MB and
   5.8 → 1.3 MB): at the edge for the release, since Cloudflare compresses `application/wasm`
   itself and doesn't properly support pre-compressed files behind a Worker (Ryan, 2026-10-25:
   no `.br` files in a build or a release, ever), and on the fly in the toolchain's own servers so
   the bench measures wire bytes; compile the WASM with `compileStreaming` so V8 can cache the
   compiled code against the resource ([V8: code caching for WebAssembly]); keep the `preload`
   links. Toolchain (the loader and the servers). Win on a 50 Mbit line: ~2 s of transfer
   (**estimated**); on a repeat visit, the WASM compile.
2. **Cut the data section.** 4.36 MB of build-time constants, 39 % zeros, copied into shared
   memory by a start function. Options, cheapest first: emit zero runs as `memory.fill`; store
   the big baked tables (`BAKED`, `OBSTACLES`, `VIEWS_GRID`, `TOUR`) as shipped files fetched on
   first use rather than `const`s; compress the segment and inflate at start. Toolchain.
3. **A first-frame pipeline set, and the rest behind the cover.** The compiler knows every
   pipeline and the frame's order is code; the runtime builds all 169 at once and the program
   stalls on the first missing one. Two halves: (a) the compiler emits each pipeline's first
   reaching site's depth in the frame (or the program names a set), and the runtime builds that
   set first and the rest while the arrival cover is up; (b) fold `const` conditions after
   inlining so `CLUSTERED`'s 17 pipelines leave the build (the open gap). Toolchain and engine.
4. **Realize through tapes at load; compile the kinds later.** The realizer's 11 instantiations
   are 2.4 MB of WGSL and the four slowest compiles. `std::stage::Tape` (`spec/language.md`
   §14.4) interprets a field as data on the GPU with derived gradient and interval, within 1e-5 of
   the compiled twin. One tape-driven `place_vertices`/`cull_blocks`/`cook_colours` set serves
   every kind for the first realization; the per-kind kernels are built after the first frame
   and used for re-realizations and skinning. Needs: a tape lowered from a compiled field at
   build time (the compiler has the field's code; "reification" is a toolchain feature), and
   tapes longer than 128 operations for creatures. **Hypothesis:** cuts the serialized compile
   by 8–10 s and the concurrent build by 2–3 s on the M4; the tape's evaluation is slower per
   point, which the one-time realize can afford. §5.2.
5. **Un-serialize the load chain.** Kinds two a frame is a knob; built things need only
   pipelines; the probes need casters, which need the wood, which needs the kinds: a dependency
   graph the planner could run critical-path first instead of in `cook_load`'s order
   (`stage.wrela:2009-2081`). The "floor ready at frame 41 / playable at 220" gap is mostly
   this chain plus the bake. **Estimated** 3–5 s.
6. **Measure a warm open.** The bench's cold run defeats caches by design; add a second-visit
   number so the claim distinguishes first and later opens (a player opens the same floor many
   times).

### 3.2 The GPU frame (the opening's 18.9 ms → under 12)

1. **Re-cook only what a tile changes.** Split the clipmap texel's height and normal from its
   cover (two textures, or two kernels over one), and on `write_fields` re-cook the **cover
   channels inside the tile's rectangle** of each overlapping level, not the whole level with
   its five height evaluations. `engine/terrain.wrela:364-407, 440-467`. **Estimated:** most of
   the 4.65 ms, and the largest share of the walk's hitch frames.
2. **Invalidate the near cascade by footprint.** A tile arriving re-draws only the strips whose
   z-band meets the tile's casters, or, better, a page-granular map (§6.1); a tile outside the
   near square (144 m) shouldn't touch it at all. `stage.wrela:1999-2004`.
3. **Grass: a prepass, a pyramid test and a kept placement.** Put the blades in the depth prepass
   (today flowers only), test grass cells against the depth pyramid like the wood does, and keep
   each ring's placement until its cell set or the wind phase changes instead of re-placing all
   five rings every frame (`engine/grass.wrela:204-402, 1270-1273, 1770-1778`). Per-blade sky
   light is sampled at placement, so a kept placement must refresh when the probes do.
4. **Cheaper cloud rays.** Fewer light samples per dense sample (8 today) with a per-cell
   transmittance table, and march at half the dome's resolution where no edge is near (the
   weather map knows). `engine/sky.wrela:433-539`.
5. **A probe clipmap.** A 30 m move re-bakes all 52,488 probes; shifting the grid and baking only
   the new columns (as the terrain and the cascades already do) removes the whole-bake from a
   walk. `engine/probes.wrela:862; frame.wrela:1875-1890`. Also trace through a **signed
   distance** volume instead of 64 fixed 0.75 m steps through density (§5.5): fields are
   distance fields; sphere tracing skips empty air.
6. **Cull what isn't culled.** The depth pyramid tests only the wood (`cull.wrela`; `wood.wrela:
   160, 205`). Terrain sides, grass cells, built things, bank stones, rocks, characters and the
   whole water mesh (drawn every frame with no in-view test, `stage.wrela:573-589`) go through
   frustum tests at best. One bounds list for everything was B4a's intent; finish it.
7. **Fewer full-screen passes.** 22 render passes, 13 full-screen, 7 of them the bloom chain, two
   full-screen copies (`frame.wrela:2234-2243; stage.wrela:566-571`). Fold lines into the
   resolve, compute the bloom's first down-sample from the resolve's output, copy once, and skip
   the water's copy when no water is in view. **Estimated** 0.5–1 ms at 1080p on the M4 and more
   on a bandwidth-bound Intel part.
8. **Early-Z for depth-writing fragments.** Impostors and the soft-raster merge write
   `frag_depth`, which turns early-Z off for those draws. WGSL's `fragment_depth` extension
   (declare `greater`) keeps it; Chrome 155–156 has it temporarily disabled on every platform
   for a Windows compiler bug, so detect it. Toolchain. **Estimated** small; free when it lands.
9. **`f16` where the compiler can prove it** (§5.3). The build compiles no `f16` path at all.

### 3.3 Smoothness (the walk's p99 29 ms → under 20)

1. **Attribute before fixing** (§3.5).
2. **Gate the far layer and slice the sky.** The far layer's 65 tiles load ungated in one frame
   (`stage.wrela:1897-1957`); the sky's noise and weather are 20 ms **estimated** each and cook
   whole (`cook.wrela:106-110`); the air's tables already cook in 8 slices through `std::job`,
   so do the same for the dome's noise and weather.
3. **Slice built things.** `realize_rigid` whole per built thing with a readback; the characters'
   `realizing` is already a `@job` yielding per slice. Use the job for built things too, and make
   the readback a frame later.
4. **One field write a frame.** The `while` that consumes every finished tile job in one frame
   (`stage.wrela:1733`) becomes one a frame, planner-gated, like placement is.
5. **Far strips at 11 ms** in Chrome: halve the strip (96 strips), or draw the far cascade at
   1024² and filter it (it is 1.17 m a texel; its job is distant canopies).
6. **Measure the planner's estimates.** `SkyNoise`, `SkyWeather`, `Kind`, `Built`, `Tile` and
   the probes are estimates; the measuring path exists (`Quality::measured`), so extend the timed
   labels to them and record the ladder's numbers in `cook.wrela`.

### 3.4 Memory (781 MiB → under 512 on the floor)

1. **A breakdown first:** `std::gpu::memory()` is counted; print it per share at the bench's
   peak, in `docs/bench.md`.
2. The known large holders: the dome at 4096×1280 Rgba16Float (42 MB; a 2048×640 dome with a
   sharper filter is 10), four 2048² depth cascades (front and back, near and far: 64 MB), the
   history and scene targets at output size, the probes' 3D occluders (161×71×161 Rgba16Float,
   15 MB), 26 impostor kinds at 3 MiB compressed (78 MiB), wood lists (64 MiB share), and the
   realizer's meshes (192 MiB share, with an eviction rule that never fires:
   `engine/memory.wrela:82-91`; name the share from the game).
3. **Stream impostor kinds:** bake a kind when its first instance enters a loaded tile, drop it
   when none remains (the plan's B6 rule, not yet wired to the wood).
4. **Alias transient targets by liveness** (§5.1): the `scene_b`, lines, mist and see-through
   intermediates and the bloom chain can share allocations; WebGPU has no placed resources, so
   the engine pools textures by (size, format) and the compiler could check liveness.

### 3.5 Tooling: the measurement gap

1. **Keep every run.** The bench writes one `results/` directory per package and the paced run
   overwrites the timed one; `timings.json` survives nowhere. Keep each run's files under
   `build/bench/<report>/<scene>/<run>/`.
2. **Name the hitch frames.** From the paced run, list each frame over 33 ms with its labels'
   GPU times, its `counters.json` row (draws, bytes written) and its `phases`; and call
   `std::time::phase` in the frame (cook, choose, passes, finish) so `phases` isn't `{}`.
3. **A pass viewer.** Test mode already has snapshots and `std::tool::drive`'s decoded
   `Commands`; a page that lists a frame's passes with their targets, lets one be shown, and
   overlays the labels' times is a studio tool (`studio/`), not a toolchain change. The plan's
   "half the time went to ad-hoc gate scripts" is the cost of not having it.
4. **Per-pipeline compile times in the bench table** (`pipelines.json` has `each`): the top
   ten, so a regression in one kernel's size is seen.
5. **A warm-open row and a per-device row** (§4.6).

### 3.6 Platform: what Chrome shipped since the plan was written

Checked against Chrome's release posts (sources at the end). Each is a toolchain feature before
it is an engine one.

| Feature | Status | What it buys here |
|---|---|---|
| Immediates (`var<immediate>`, `setImmediates`) | Chrome 149–150 | Per-draw and per-dispatch small data without the uniform ring's 256-byte slots and bind-group churn; 116 compute pipelines a frame make many small uniform writes |
| `uniform_buffer_standard_layout` | Chrome 144 | Smaller uniform blocks: no 16-byte padding of array elements and nested structs |
| Faster `writeBuffer`/`writeTexture` | Chrome 144, "up to 2×" | Already in the browser; the upload ring's staging copy was tried direct and reverted, so re-measure A13.1 |
| `TRANSIENT_ATTACHMENT` | Chrome 146 | Tile-memory attachments never written to VRAM: only for an attachment nothing reads later (an MSAA target, a pass-local depth); little here today |
| Subgroup size control, `subgroup_id` | Chrome 144, 151–152 | Fixed subgroup width for the prefix sums and compaction; `subgroups` itself shipped in 2025 |
| `buffer_view`, `swizzle_assignment`, `linear_indexing` | Chrome 147–154 | Reinterpreting buffer bytes without a copy (the command stream's decoders; packed vertex formats) |
| `fragment_depth` (conservative depth) | 155–156, temporarily disabled | Early-Z for the impostors' and the soft-raster merge's depth writes |
| `texture-compression-unaligned` | 155–156 | BC atlases not a multiple of 4 |
| `srgb-linear` and `display-p3-linear` canvases | 155–156 intent to ship | An HDR and wide-gamut output path on XDR displays (§7.6) |
| Bindless | 155–156, early experimentation (Android, Linux, Windows) | Material and texture arrays without per-draw bind groups; not on macOS yet |
| Compatibility mode (OpenGL ES 3.1, D3D11 next) | Chrome 146, Android first | Reach to GPUs without Vulkan, Metal or D3D12, with a restricted feature set; a second rendering tier if the reach is wanted |
| 64-bit atomics, subgroup matrices, texture atomics | Not shipped (roadmap; subgroup matrix experimental) | The soft raster stays two-pass; neural work stays on `f16` + subgroups |

Browsers: Firefox ships WebGPU on Windows (141) and on Apple-silicon macOS (145), with Linux and
Android in progress; Safari 26 ships it with no `subgroups` (caniuse). So every subgroup and `f16`
path needs its fallback measured (`--features none` exists for this).

WebAssembly: the build uses standard SIMD, threads, bulk memory and multivalue at `-Oz`, and
forbids relaxed SIMD everywhere because `@deterministic` code must be bit-exact. Relaxed SIMD
(fused multiply-add, relaxed lane ops) is allowed by the spec outside deterministic code; the
compiler knows which functions are deterministic, so **relaxed SIMD in presentation code only**
is a toolchain feature with a CPU win on the frame's 4–5 ms (**estimated**, unmeasured).

## 4. What "world class" lacks here, against the AAA list

The engine's tier (dynamic sun, probes, local lights, moving shadows, dense vegetation, SSR,
particles, TSR, GTAO, contact shadows, exposure, bloom, grading) is a 2015-era deferred renderer's
feature list done in forward with cooked fields. What a 2026 AAA forest renderer has that this
one doesn't, in the order the stills complain:

1. **Rock as a first-class surface.** Overhangs, strata, cracks, lichen, wet darkening, and
   near-camera displacement. Here: a heightfield with solids and one bark-like noise.
2. **Water as a system.** Flow maps along the courses, waterfalls with foam columns, spray,
   mist and wet margins, planar or probe reflections for the mere, caustics in shallows,
   shoreline waves. Here: one static mesh, two ripple noises, SSR, foam at banks.
3. **Light in the air.** Froxel fog with the sun scattered through the canopy and the cloud
   shadows, god rays that follow the trees, height fog in the hollows at dawn.
4. **Shadows that don't leak or swim.** Page-cached, 16k-effective, with proper filtering (PCSS
   or a moment variant) and no replacement visible when a tile lands.
5. **Indirect light with visibility.** Bounce that stops at a wall; a tree's underside darker
   than its crown; interiors.
6. **Animation.** Clip blending, motion matching or learned locomotion, ragdoll, cloth and hair
   with shading models (sheen, strands). Here: procedural gait, two-bone IK, PBD cloth drawn flat.
7. **Materials.** Layered materials with field-derived masks (curvature, cavity, thickness are
   listed in the vision as cooked inputs), anisotropy, clear coat, subsurface for leaves and
   skin, parallax, and detail at a metre.
8. **Image.** HDR output, depth of field for the shots, motion blur for the duel, lens effects
   under the look's control, film-grade grading.
9. **Scale.** Texture and geometry streaming by distance (impostor kinds, cluster pages),
   occlusion for everything, a memory budget that is enforced.
10. **Reach.** Any evidence on Intel, AMD, Apple A-series, Firefox or Safari.

Several are content and look decisions that are Ryan's; the engine's job is to make each
possible within the budget, and today 1, 2, 3, 5, 8 and 9 aren't possible at all.

### 4.6 The device matrix and the competitors

The claim (`docs/plan-world-class.md` §0.1) names integrated GPUs and native engines; the evidence
is one M4 Air and no competitor. Because the sim is bit-exact and the command stream is hashed,
**a recorded walk replays identically on every device**: the device matrix is the same tick log
and the same `wrela tool bench` on each machine, with the pixels compared within a tolerance and
the spans tabled. The matrix that matters for "commodity hardware":

| Device | Why |
|---|---|
| MacBook Air M1, 8 GB | The oldest Apple silicon still common; half the M4's GPU; shared 8 GB |
| An Intel Iris Xe laptop (12th-gen, 16 GB), Windows, Chrome and Firefox | The commodity PC; D3D12 through Dawn; no `f16` on some drivers |
| An AMD Radeon 780M or 890M handheld or laptop | The other integrated PC GPU; Vulkan or D3D12 |
| An iPad Pro (M-series) in Safari 26 | No `subgroups`; a different compiler; touch |
| A Pixel or Galaxy with Chrome | Adreno or Mali, compatibility mode's home turf; phones are a non-goal, but one number says how far the engine is from them |

Competitor scenes (the sibling repository, still to make): the same forest brief in three.js
WebGPU, Babylon.js and PlayCanvas (which has the most production WebGPU use among the web
engines), and a Unity 6 Web build if a licence is to hand. Each measured with the same harness
shape: GPU span at 1080p, paced intervals, cold start, memory. Without them "a visual tier native
engines run under 30 fps" is a sentence.

## 5. What only a compiler that sees the game can do

The vision's fourth thesis says the wins are "generality". These are the places where the
whole-program view turns into frame time, bytes or seconds, each a **hypothesis** with the
experiment that tests it. All need toolchain work; the layering rule holds: each is phrased as a
general language or GPU capability, and none names a tree.

### 5.1 A frame plan the compiler checks

The frame's passes, their targets, formats and sizes are fixed by code for a given program; the
manifest already lists every render pipeline's targets. What the engine lacks is the step a
render graph does: alias transient textures by liveness, order passes to shorten the chain, and
know which target a later pass reads. A `std::gpu` "plan" that a program records once (a list of
passes with their reads and writes) lets the runtime allocate targets from a pool and the
compiler check that no pass reads what it writes. The win is memory (the intermediates) and a
little bandwidth; the bigger payoff is that §3.2.7's pass folding becomes a check, not a hope.

### 5.2 Fields as data: reify a compiled field to a tape

`std::stage::Tape` already runs a field built at run time on both targets with derived gradient
and interval. The compiler can lower any compiled `Surface` to the same tape form at build time,
because it has the code. Then:

- one tape-driven extraction pipeline set serves every kind at load (§3.1.4);
- a creature edited in the lens re-realizes through its tape with no rebuild and no new pipeline;
- a tape can be **pruned per block** (`tape.prune(over)`, §14.4) before extraction, so a 4 cm
  block of the tower evaluates the two parts that reach it, not all of them: Keeter's MPR (2020)
  and Fidget are the precedents, and the engine's per-part masks for creatures are the local
  form of it.

Needs: tapes beyond 128 operations, tape lowering in the compiler, and a measurement of
interpretation against compiled evaluation on the M4 and on an Intel part.

### 5.3 Precision from intervals: `f16` where it is proven safe

The compiler derives the interval of every value a function computes over a box (§14.2), on the
GPU with WGSL's error bounds included. That same derivation says where a sub-expression's range
and required precision fit `f16`. A pass that lowers proven-safe arithmetic to `f16` under
`features().f16` (with the `f32` twin kept for devices without it, as `@gpu(needs:)` already
allows) doubles ALU rate and halves register pressure on Apple and Intel GPUs for exactly the
code that dominates: field evaluation in the realizer, the terrain cook, the grass placement, the
cloud march and the probes' rays. No pipeline in the Last Green uses `f16` today. The experiment:
the clearing's `cook_level` and `place_vertices` in `f16` by hand, `--features none` against
`f16`, pixels within 1/255.

### 5.4 Constants folded into shaders

`.bind(x: const)` is a uniform, not a specialization; `Quality`'s switches are uniform branches
in shaders; `const` conditions aren't folded after inlining (the `CLUSTERED` gap). A shipped
game's `Quality` is a constant. Specializing pipelines on constant arguments (WGSL `override`s
or monomorphization on the value) removes dead branches, uniforms and, for the Last Green, 17
pipelines; in the other direction (§5.2) the compiler should *stop* specializing where a type
parameter only changes data. "Specialize on constants, generalize on fields" is one feature
with two directions.

### 5.5 Distance, not density: cooked signed-distance volumes

The probes march a *density* volume in 64 fixed steps of 0.75 m (`probes.wrela:291-364`). Fields
are signed distance functions; cooking a signed-distance volume of the near world (the trunks,
rocks, built things and the ground as a heightfield distance) lets every ray sphere-trace: empty
air in a few steps, surfaces found exactly. The same volume gives contact shadows, soft shadows
for the moving map, ambient occlusion away from the screen (the vision's "sky occlusion traced
through cooked fields"), and the global SDF that Lumen's software path uses for GI, which is the
production precedent for "trace cooked distance fields on any GPU". The compiler's part: the
derived `interval` certifies the volume's cells (which block holds a surface) as spike 13 did for
meshes, so the cook is exact rather than sampled.

### 5.6 Continuous LOD from the field itself

The cluster path's error is a heuristic and its levels are separate extractions; the plan's group
DAG was never built. A field offers something a mesh can't: **re-extraction at any resolution and
exact projection of any vertex onto the surface**. Extract level k+1's grid, project level k's
vertices onto the field along its gradient, and every vertex has an exact morph target one level
up; morph by distance and no crack or pop can appear, with no DAG, no locked boundaries and no
skirts. Hable's 2026 compute tessellation (SIGGRAPH Advances) does the screen-adaptive half for
meshes; the field does the correctness half for free. This replaces "whole-scene virtual
geometry" as the geometry bet: it fits rock, masonry, built things and creatures, and it is
weeks, not months, because the extraction and the gradient already exist.

### 5.7 What moves, from data flow

`temporal.wrela:10-11` says "nothing here knows what moves"; motion vectors are per-object only
for characters and grass. The compiler knows which draws read the sim's snapshot and which read
only the camera: a draw is static or moving by data flow. That classification gives dilated
per-object motion vectors for everything that moves, a static-caster list for shadow caching, and
a reactive mask for the resolver without the author marking anything.

### 5.8 Static scheduling of the cooks

The planner measures costs at run time on a ladder, from hand estimates (`e(20000)`). The
compiler can emit a cost model per kernel (instruction count × domain) as a better prior, and
the cooks' dependency graph is code (air → dome → probes; kinds → wood → casters → probes). A
static critical-path schedule for the load (§3.1.5) and better priors for the frame are the
same feature.

### 5.9 Determinism as a laboratory

Bit-exact replay on every host is already true. Three uses the engine doesn't make yet: the
device matrix (§4.6) as one script on many machines; **differential tests** of rendering
changes by pixel difference on a replayed walk rather than stills; and leaderboard replays as
regression tests for the sim's cost (the "sim ≤ 4 ms per tick" budget is unmeasured on the
Last Green in `docs/bench.md`).

### 5.10 Caches that survive a release

The compiler knows each pipeline's WGSL and its hash. If the WGSL text is stable across releases
(deterministic binding order, no source positions in names), Dawn's cache serves the pipelines
an update didn't change. The same holds for baked artefacts in OPFS (impostor atlases, probe
volumes, realized built things) keyed by the field's hash and the device: content-addressed,
so an update re-bakes only what changed. Measurement first (§3.1.6): nobody has timed a warm
open.

### 5.11 Training at build time

The build computes constants by running the program's own code (§13.1). That is a training
loop with no new machinery: a `const` can be the weights of a small network fitted to a field's
cooked channels, an impostor's views or a material's response, computed once on the developer's
machine and shipped as numbers. The frontier uses are in §7.

## 6. The cutting edge that fits an integrated GPU

What 2025–2026 production renderers do, filtered by "runs under 2 ms at 1080p on an M4 or an Iris
Xe, in WebGPU without 64-bit atomics, bindless or ray-tracing hardware". Each names the prior art
and what the engine already has that it builds on.

### 6.1 Page-cached clipmapped shadows (virtual shadow maps)

The cascades are already strip-replaced caches. The next form is Epic's VSM shape without sparse
textures: a physical page atlas, a page table per clipmap level, pages marked needed from the
depth buffer each frame, drawn only when missing or invalidated, and invalidated by the
footprint of what changed (a tile's bounds, a moving caster's bounds), with the moving casters
drawn into separate pages over the cached static ones. Prior art outside Epic: UnityVSM (GPU
Zen 3), Sakmary's 2025 thesis (Vulkan), Blender EEVEE-Next. Fixes: the tile-arrival replacement
(§2.2), the 11 ms far strips, the 64 MB of cascades, and gives 16k-effective resolution under
the eye. Filtering: PCSS or a moment variant after the pages, since soft edges hide page seams.

### 6.2 GI in three steps

1. **A visibility term for the probes** (DDGI's depth and depth² per probe direction): stops the
   leaks and makes interiors possible; the probes already trace rays, so it is storage and a
   lookup change. Cheap, and the prerequisite for everything below.
2. **Screen-space probes with world-space intervals** (radiance cascades): the Chalmers 2025
   theses show screen-space RC at a few milliseconds without ray-tracing hardware; **Split
   Radiance Cascades** (Freeman and Sannikov, arXiv 2607.20384, July 2026) moves them to sparse
   world-space probes in a hashmap with ray splitting. Rays here march the cooked distance volume
   (§5.5). This is the first GI technique designed around not having temporal history to lean on.
3. **Production references for the budget:** id Software's idTech 8 (SIGGRAPH 2025) went from
   baked to real-time GI at 60 Hz on all platforms, probe-based; Ubisoft's *Assassin's Creed
   Shadows* (2025) convolves probes into a filtered radiance cache to trade correctness for
   stability in dense vegetation. Both say: probes, filtered, updated by priority.
4. **Variable-rate updating** (Infinity Ward's variable rate ray tracing, SIGGRAPH 2026): a
   temporal gradient steers the probe re-trace to columns where the sun, clouds or casters
   changed, instead of the uniform golden spiral; the total ray count stays constant per frame.

### 6.3 Light in the air

A froxel volume (Wronski 2014; Hillaire 2015, Frostbite) at 160×90×64 over 200 m: scattering per
froxel from the sun through the cascades and the cloud shadow, the probes for ambient, a local
height fog field a floor authors; integrated front to back; applied to every lit pixel with the
aerial-perspective LUT taking over beyond it. The shafts' quarter-resolution march becomes a
special case. IO Interactive's *Smolder* (SIGGRAPH 2026) is the current production framework
for layering such volumes with opaque, transparent and fog layers. **Estimated** 0.6–1.0 ms at
1080p on the M4.

### 6.4 Many lights, stochastically

The froxel light list is there (256 lights, 32 a froxel, unshadowed). For the floors that need
fire, lanterns or a town: HypeHype's stochastic tile-based lighting (SIGGRAPH 2025) runs fully
dynamic local lighting *with shadows* at fixed cost on low-end mobile GPUs by sampling a few
lights per tile and sharing samples; Epic's MegaLights is the same idea at the high end. The
engine's TSR already gathers stochastic results (GTAO relies on it).

### 6.5 Water that moves

Flow maps baked from the courses' direction fields (the courses are fields), advected normals and
foam; wave particles at the falls' feet; the falls themselves as a column of sorted particles
plus a mist froxel contribution; shoreline wetness from the distance field. Reflections: a planar
reflection for the mere's still water (one extra pass at half resolution when it is in view) and
SSR elsewhere. None of this is new research; all of it is what the waterfall still is missing.

### 6.6 Transparency and foliage

Weighted blended OIT is in. Activision's adaptive voxel OIT (2025) is the next step only if
layered smoke or glass appear. For the canopy, the question is not OIT but **leaf light**: a
backlit transmission lobe against the sun with thickness from the card's field, and the far
crowns' volume way lit by the probes with visibility.

### 6.7 Upscaling

The resolver is a hand-made FSR-2-class design missing three of its parts: dilated motion vectors
(the nearest-depth 3×3), a reactive mask for the cards and particles, and a luma-based lock for
thin features (grass, far trunks). Sony's 2026 PSSR rework (SIGGRAPH 2026) is the useful
reference on where learning helps: closed-form everywhere it works, a small network only for the
pattern recognition (history rejection), which keeps the cost in reach of an integrated GPU.
Intel's own budget for a DP4a upscaler on an iGPU was under 5 ms at 1080p; a learned rejector on
top of a closed-form resolve should cost a fraction of that (**estimated**; §7.4).

### 6.8 Geometry

The 2026 evidence says software rasterization pays only at hundreds of millions of triangles
(CuRast, April 2026: 2–5× over Vulkan there, an order of magnitude slower on low-poly meshes),
which B7a's measurement anticipated. On the web, `three-virtual-geometry` reports 5.7 billion
triangles at 160–230 fps on an M4 Pro in Chrome 154 with hardware raster only. The scenes here
are nowhere near that; the geometry bet should be §5.6's field LOD, hardware-rasterized, plus
compute tessellation for the clipmap's near rings (Hable 2026) so the crag's edge stops
terracing.

### 6.9 Hair and strands

MachineGames' strand-based hair (SIGGRAPH 2025) is the production reference when the fawn's fur
or the climber's fringe becomes a goal; the vision's spike 11 found raster plus depth-bounded
tracing viable at ~1 ms. Later.

## 7. The frontier

Ideas with no production precedent in a browser, each with the first experiment.

### 7.1 Neural impostors

An impostor kind is a 1024² octahedral atlas of 256 views in colour and normal (`impostor.wrela:
7-11, 130-150`), 3 MiB compressed, baked at load. A small network per kind (view direction and
texel → colour, normal, depth, coverage) fits in a few kilobytes of weights, trained **at build
time as a constant** (§5.11) from the same bake, evaluated in the impostor's fragment shader in
`f16` with subgroups. Intel's 2025 neural block texture compression decodes latents with an MLP
at 23× the speed of plain FMA with cooperative vectors; without them (no subgroup matrix in
WebGPU yet), Google Meet measured 2.3–2.9× for matrix-vector work from subgroups alone. The
experiment: one kind, one network, its shader's cost against the atlas lookup at 1080p on the
M4, and its memory. If it pays, 26 kinds of impostors become kilobytes and the far forest is
view-continuous with no atlas seams. **Hypothesis.**

### 7.2 Neural materials from the field

Bark, rock and masonry are fields evaluated into the cooked channels. A per-material network
fitted at build time to the field's response over (position, normal, view) gives layered,
anisotropic materials at a texture lookup's cost, which the vision measured as the thing
per-pixel fields can't do (1.6–2.8× a texture). Same machinery as 7.1.

### 7.3 A neural radiance cache for the probes' bounce

Müller et al.'s neural radiance cache (SIGGRAPH 2021) learns indirect radiance online from the
renderer's own rays. The probes' rays are that signal; a small network queried per probe (not
per pixel) would give multi-bounce light where the SH store gives one. Train on the GPU in the
same frame budget as "probes again". **Hypothesis**; needs `f16` and a measured training step.

### 7.4 A learned history rejector for the resolver

The resolver's disocclusion and clipping are heuristics with known artifacts (creature trails,
edge smear, dither screens over crowns, `temporal.wrela:47-55, 328-346`). A network of a few
hundred weights, trained at build time on the engine's own frames against a supersampled truth
(the stills pipeline makes both), replaces the rejection decision only. Sony's PSSR rework is
the argument that this is where learning belongs.

### 7.5 Build-time training as a std facility

7.1–7.4 share one need: a differentiable loss computed by the program's own code at build time.
The compiler already derives gradients of any function (§14.1), including by literals
(`std::lift::gradient`). A `std::fit` that runs gradient descent over a `const`'s parameters,
checked by the same derivation, is a language feature a non-game program would want (a filter
fitted to a response, a controller's gains), so it passes the layering rule.

### 7.6 HDR and wide gamut

Chrome 155–156 brings `srgb-linear` and `display-p3-linear` canvases. The frame is scene-referred
until `develop`; an output transform to P3 with headroom on an XDR display is a toolchain canvas
feature and a `Look` setting, and it is a visible marker of a serious renderer.

### 7.7 Far forests as splats

The far crowns are 24³ density volumes per kind drawn as quads. Gaussian splats have no shipping
foliage use (LeafFit, EG 2026, converts captures to meshes; PlayCanvas and Wicked render raw
splats), but a splat per far crown cluster, lit by the probes, is a plausible replacement for the
volume way with better anti-aliasing and a continuous handover from impostors. Low priority;
listed so it isn't rediscovered.

### 7.8 Rollback multiplayer

Out of the engine's scope until a game wants it; what the engine would owe it: input delay as a
`Timeline` parameter, snapshot serialization with a size bound (the world is `Serialize`), and
a prediction window measured against the 4 ms tick budget.

## 8. What I'd do, in order

Each line is a plan's step with a gate, in the shape of `docs/plan-world-class.md`. The order puts
measurement before claims, the cheap and certain before the expensive and hypothetical, and the
toolchain features in the releases that the engine steps need.

1. **Tooling that can name a hitch** (§3.5): keep every run; hitch frames with labels and
   counters; `std::time::phase` in the frame; per-pipeline compile times; a warm-open row. A week.
   Gate: the walk's nine frames over 33 ms each have a named cause in `docs/bench.md`.
2. **Cold start to 5 s on the M4** (§3.1): compression and streaming compile (toolchain
   0.8.0; 0.7.0 shipped on 2026-10-25 without any of these), the data section, the first-frame pipeline set and the folded constants (0.8.0), the
   load chain un-serialized, the realizer through tapes (0.8.x) if 1–3 don't reach 5 s alone.
   Gate: playable within 5 s cold and 3 s warm, `docs/bench.md`.
3. **The re-cook and the hitch sources** (§3.2.1–2, §3.3): tile-rectangle cover re-cooks; the
   near cascade's footprint invalidation; the far layer gated; the sky sliced; built things as
   jobs; one field write a frame; measured planner costs. Gate: the opening's GPU frame under
   14 ms and the walk's p99 under 20 ms with no frame over 33 ms in 1,200.
4. **The rest of the frame** (§3.2.3–8): grass prepass, pyramid and kept placement; culling for
   every kind; pass folding; cloud rays. Gate: the opening under 12 ms, 20 % headroom for the
   M4's clock.
5. **The device matrix and the competitors** (§4.6): the replayed walk on an M1 Air, an Iris Xe
   laptop, a 780M and an iPad; three competitor scenes. Gate: `docs/bench.md` has the rows, and
   the claim's wording (D9) is rewritten to what they show.
6. **Memory to 512 MiB on the floor** (§3.4): the breakdown, the dome, the cascades, streamed
   impostor kinds, the realizer's eviction. Gate: the M1 Air's 8 GB holds the floor with Chrome's
   other tabs open.
7. **The look, engine side** (§6.1–6.5, §1.3): page-cached shadows; the probes' visibility term;
   the froxel volume with canopy scattering; the signed-distance volume; field LOD and compute
   tessellation for rock; water's flow, falls and particles; cloth shading; the resolver's three
   missing parts. Each behind a `Quality` switch, each with stills in the PR. Gate: each
   lookbook shot in §1.3 is re-judged by Ryan, and the opening stays under 14 ms with every switch
   on.
8. **The compiler's advantages** (§5.3, §5.4, §5.7, §5.8; toolchain 0.9): `f16` by intervals,
   constants folded into shaders, static-or-moving by data flow, cost models. Gate: measured
   wins per item on the M4 and the Iris Xe, kept or reverted.
9. **The frontier** (§7): build-time fitting as a std facility, then neural impostors as the
   first experiment, then the rejector, then radiance cascades over the distance volume. Gate:
   each a measured experiment in `docs/bench.md`, shipped only if it pays.

Items 1–4 are weeks and close the claim on this machine; 5–6 make the claim mean something;
7 is what a player sees; 8–9 are what no other browser engine can do.

## 9. Open questions for Ryan

- **The claim's wording.** "99th percentile under 16.7 ms" can't hold in headless Chrome (the
  leapling's is 18.2 ms); `docs/bench.md` already judges by GPU frame and frames over 33 ms.
  Restate D9 as: GPU frame under 12 ms at 1080p, no frame over 33 ms in 1,200, playable within
  5 s cold on the M4 Air and the M1 Air, and under 25 ms with none over 50 on an Iris Xe.
- **Which floors' looks need which features** (lights, fog, water) decides the order inside
  item 7.
- **Reach:** whether compatibility mode, Safari and Firefox are gates or records.
- **The cluster path:** keep it as an example and a test, or remove it from the engine until
  §5.6 replaces it.

## Sources

Code and runs: `docs/bench.md` (B13 and the repeats), `docs/plan-world-class.md`,
`docs/toolchain-gaps.md`, the saved bench build under `build/bench/builds/last-green-walk/`,
`games/last-green/build/manifest.json`, `wrela pipelines games/last-green --json`, the stills under
`build/stills/b13/`, and the files and lines named in the text; the toolchain at
`rywible/wrela-compiler` tag `v0.6.3` (`runtime/browser/src/gpu.ts`, `worker.ts`, `program.ts`,
`loader.ts`, `compiler/driver/src/optimize.rs`, `spec/language.md`).

Platform:
- Chrome for Developers, *What's New in WebGPU*: [117](https://developer.chrome.com/blog/new-in-webgpu-117) (pipeline caching with auto layout), [144](https://developer.chrome.com/blog/new-in-webgpu-144) (`uniform_buffer_standard_layout`, faster `writeBuffer`), [146](https://developer.chrome.com/blog/new-in-webgpu-146) (compatibility mode, `TRANSIENT_ATTACHMENT`), [147–148](https://developer.chrome.com/blog/new-in-webgpu-147-148), [149–150](https://developer.chrome.com/blog/new-in-webgpu-149-150) (immediates), [151–152](https://developer.chrome.com/blog/new-in-webgpu-151-152) (subgroup size control), [153–154](https://developer.chrome.com/blog/new-in-webgpu-153-154) (`buffer_view`), [155–156](https://developer.chrome.com/blog/new-in-webgpu-155-156) (bindless preview, `fragment_depth`, linear canvases); [What's next for WebGPU](https://developer.chrome.com/blog/next-for-webgpu) (64-bit atomics, bindless, subgroup matrices).
- Firefox: [WebGPU on Windows in Firefox 141](https://simonwillison.net/2025/Jul/16/webgpu-firefox); [web.dev, WebGPU in major browsers](https://web.dev/blog/webgpu-supported-major-browsers). Safari: [caniuse, WebGPU subgroups](https://caniuse.com/wf-webgpu-subgroups).
- V8, [Code caching for WebAssembly developers](https://v8.dev/blog/wasm-code-caching). WebAssembly 3.0 (June 2026): [byteiota](https://byteiota.com/webassembly-30-spec-release/) (secondary).
- Subgroups and ML: [Chrome 134 post](https://developer.chrome.com/blog/new-in-webgpu-134) (Google Meet's 2.3–2.9×); [web3dsurvey, subgroup matrix](https://web3dsurvey.com/webgpu/features/chromium-experimental-subgroup-matrix).

Techniques:
- Virtual shadow maps: [UnityVSM](https://github.com/octoon/UnityVSM); [Sakmary, *Virtual Shadow Maps*, CTU 2025](https://dcgi.fel.cvut.cz/en/theses/2025/sakmamat/); [Resolution-matched VSMs, CESCG 2025](https://cescg.org/wp-content/uploads/2025/04/Sakmary-Resolution-Matched-Virtual-Shadow-Maps.pdf).
- Radiance cascades: [Freeman and Sannikov, *Split Radiance Cascades*, arXiv 2607.20384](https://arxiv.org/abs/2607.20384); [Chalmers 2025, screen-space RC in 3D games](https://odr.chalmers.se/items/9e42007a-bb60-440f-ba28-04410822758f/full); [Chalmers 2025, exploration and optimization](https://odr.chalmers.se/items/3ee9fb4e-1880-46c2-802a-a660e38dc9ee).
- SIGGRAPH 2025 *Advances in Real-Time Rendering in Games* ([course page](https://advances.realtimerendering.com/s2025/index.html)): Sousa, *idTech 8 GI*; Lempiäinen, *Stochastic Tile-Based Lighting in HypeHype*; Drobot, *Adaptive Voxel-Based OIT*; Leblanc and Conte, *Ray Tracing the World of Assassin's Creed Shadows*; Narkowicz and Costa, *MegaLights*; Kulikov, strand hair.
- SIGGRAPH 2026 *Advances* ([course page](https://advances.realtimerendering.com/s2026/index.html)): Hable, *Adaptive Tessellation and Subdivision*; Craig, *Upgrading PSSR*; Olejnik, *Variable Rate Ray Tracing*; Mueller, *Smolder*; Greenberg, *ORCA*; Makeev, *SLIM*.
- Neural: [Belcour and Benyoub, neural block texture compression with cooperative vectors, arXiv 2506.06040](https://arxiv.org/pdf/2506.06040); [SIGGRAPH 2025 papers list](https://www.realtimerendering.com/kesen/sig2025.html) (*Kernel Predicting Neural Shadow Maps*); Müller et al., *Real-time Neural Radiance Caching for Path Tracing*, SIGGRAPH 2021 (from memory; verify).
- Geometry: [Scthe, nanite-webgpu](https://github.com/Scthe/nanite-webgpu); [three-virtual-geometry](https://cdn.jsdelivr.net/npm/three-virtual-geometry@0.1.3/README.md); [CuRast, arXiv 2604.21749](https://arxiv.org/pdf/2604.21749); [LeafFit, EG 2026](https://arxiv.org/abs/2602.11577).
- Upscaling budgets: [Intel, XeSS on DP4a (2023)](https://game.intel.com/wp-content/uploads/2024/02/GameDev2023_XeSS_v1.pdf).
- Lumen's software path: [Epic, Lumen technical details](https://dev.epicgames.com/documentation/unreal-engine/lumen-technical-details-in-unreal-engine).
- Already in `docs/citations.md` and relied on above: Majercik et al. (DDGI), Schneider (Nubis), Hillaire (sky), Karis (Nanite, TAA), Haar and Aaltonen, Keeter (MPR, in the vision's prior art). Froxel fog: Wronski, *Volumetric Fog*, SIGGRAPH 2014; Hillaire, *Physically Based and Unified Volumetric Rendering in Frostbite*, SIGGRAPH 2015 (both from memory; verify before citing in code).
