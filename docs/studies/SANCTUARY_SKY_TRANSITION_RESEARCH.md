# Sanctuary sky-transition cache research

## Problem measured

The six-second native Sanctuary climate study at 1920×1080, 4x MSAA, on Apple
M4 is recorded in
`.build/sanctuary-native-20260912/climate-native-study.json`. Its transition
section reports a `rebuild` GPU maximum of **203.199 ms**, aggregate GPU p95 of
**14.400 ms**, and a presented-frame maximum of **199.999 ms**. The 200 ms
stall is therefore a cache-rebuild frame, not steady scene rendering. It is not
acceptable as the cost of crossing an automatic climate boundary.

The 203.199 ms figure is the transition sample at frame 675. The same report's
normal sliced work is materially smaller: sky-phase p95 reaches 14.407 ms and
lighting-phase p95 13.088 ms. Earlier capture sections in the file retain
separate startup rebuilds up to 317.375 ms; those are not substituted for the
transition measurement above.


## Implemented decision and September 12 validation

The implementation below is now in the renderer; the original proposal that
follows remains design history, not a claim that every proposed API or lifetime
mechanism was adopted. Native checks were run by the root agent with one Wrela
render app at a time. This review read the recorded artifacts and ran no GPU work.

### Implementation actually retained

- `Atmosphere` keeps typed cache signatures and generation checks. One candidate
  captures its uniforms, sample time and immutable weather snapshot. One CPU
  weather job may run at a time; cancellation drains it before a replacement.
- Live compute advances one acknowledged unit per frame: one density/reduction/
  empty-cell Y slice, one sunlight-order lighting slice, 16 air rows, two or four
  sky rows, or a small LUT/irradiance pass. A completion mailbox gates later work.
  Only successful completion of the final step for the desired generation swaps
  texture references. Stale requests retire without changing publication times.
- One spare auxiliary set and one spare density hierarchy are allocated during
  atmosphere construction. The existing three panorama/irradiance pairs are
  reused. Metal command-buffer retention and an explicit frame reference retain
  presented textures; the serial queue orders scratch reuse after prior reads.
  This uses bounded preallocation rather than the proposal's lazy cache sets.
- The frame binds one completed texture snapshot. A small `OutdoorShadow` CPU
  description supplies the current shadow center and projection dimensions.
  `FrameComposer.outdoorShadowMatrix` uses the published cache sun while retaining
  the current camera-dependent shadow region. Sanctuary and Soundstage supply
  this contract. Direct lighting, the solar disk, haze and other atmospheric
  shader inputs now use the same published signature. Camera, exposure, wetness
  and scene look remain per-frame inputs. No game state or new uniform ABI was
  added to the renderer.
- Startup and explicit paused capture still use the synchronous exact path.
  `verifySkyCache` is an explicit blocking diagnostic: it retires an in-progress
  candidate, rebuilds cold at the **published** sample time into existing scratch
  textures, compares GPU panorama/irradiance outputs, and restores active
  textures, times and weather. The native command temporarily pauses without
  allowing an intervening draw, so it can compare a live publication rather than
  first replacing that publication with a current-time paused rebuild.

This repairs live scheduling and pending-cache lighting consistency. Publication
still changes the current sun discretely when construction finishes. The existing
sky/irradiance cross-fade remains, so this is not a smoothly interpolated complete
scene-light transport solution. Long construction and cache age remain visible
quality/latency tradeoffs rather than being hidden by synchronous work.

### Final profiled native evidence

Primary artifact:
`.build/sanctuary-native-20260912/coherent-game-cache.json`. Both windows use
Apple M4, native 1920×1080, 4x MSAA, profiling enabled, thermal state 0 and low
power mode disabled. The fixture runs production rendering, weather and
population while stationary; only the initial saved age is relocated to the
climate boundary, and the interpreter is disabled. The windows lasted 18.031 s
and 18.405 s, with 1,078 and 1,101 whole-frame GPU samples respectively. There
were no recorded GPU errors.

| Inclusive live metric | Automatic morning→noon | Rapid golden→sunset→noon | Original threshold |
| --- | ---: | ---: | ---: |
| GPU p95 | 14.292 ms | 14.626 ms | ≤16.7 ms |
| GPU maximum | 19.496 ms | **26.025 ms** | ≤25.0 ms |
| Presented interval p95 | 16.667 ms | 16.667 ms | ≤20.0 ms |
| Presented interval maximum | 33.333 ms | 33.333 ms | ≤33.4 ms |
| Atmosphere CPU maximum | 0.142 ms | 0.141 ms | Reported, not separately budgeted |

These GPU numbers include ordinary candidate work; they are not a subtraction
of candidate time from the active frame. Both presentation thresholds and both
GPU p95 thresholds pass in this artifact. The rapid window still fails the
25 ms GPU maximum by 1.025 ms. The complete original acceptance plan is **not
passed**: the required three matched repetitions and a separate matched
no-change comparison have not been established by this pair of windows.

All **106/106 sampled statuses** report `sceneSunLeadsCache == false`. This
includes 52 statuses where the requested sun differed from the active cache
(17 automatic, 35 rapid). This supports the new pending-cache light policy at
those sampled frames; it is not an exhaustive image comparison of all frames.

Automatic generation 2 was requested at frozen sample time 1.500 s and was first
observed published at window elapsed 7.843 s. The next candidate's sample time
7.883 s implies about **6.38 s** of construction for that changed-sun cache;
status polling only brackets the publication event. The rapid sequence requested
3, 4 and 5. Generation 2 and its published snapshot time 7.883 s remained active
while 3/4 were retired. Only generation 5 was published, first observed at window
elapsed 13.006 s. Its frozen sample time was 19.900 s; the following cycle began
at 30.816 s, implying about **10.92 s** of construction for the density-changing
candidate. At the final rapid sample the current cache was already 16.683 s old
while a subsequent refresh was still building. These are meaningful latency
limits despite bounded frame work.

### Remaining outlier: what the counters do and do not show

The 26.025 ms rapid maximum is recorded under `waiting`: that frame encoded no
candidate compute unit because a prior step was still awaiting completion. It
first appears between the 0.399 s and 0.791 s samples around the golden request.
The same interval records 23.668 ms maximum drawable acquisition and 42.171 ms
maximum submission-to-completion latency. The sampled `shadow.vertex` maximum
rises from 3.332 to 5.743 ms in this reporting interval. Other stage maxima are
smaller, but the counters are sampled every fifth frame and retained as aggregate
statistics, without per-outlier frame linkage. That correlation does not prove
that the shadow pass caused the exact 26.025 ms command-buffer interval or the
drawable wait.

The rapid candidate-density stage counter maximum is **1.917 ms**, and the
candidate-sky stage maximum is **2.684 ms**. Whole frames actually labeled
`candidate.density` peak at **14.967 ms**; `candidate.sky` frames peak at 15.110 ms.
The evidence therefore does not identify an oversized candidate slab as the
remaining maximum. It supports further frame-linked submission/presentation
investigation before changing the slice budget. Stage intervals overlap and
have **not** been added to reconstruct frame time.

The earlier artifact
`.build/sanctuary-native-20260912/cache-publication-native.json` remains part of
the record. Its automatic window reached 16.667 ms maximum presentation, but its
first rapid golden request produced **83.333 ms** maximum presentation,
58.938 ms maximum drawable waiting and 25.892 ms maximum GPU time, again under
`waiting`. That run had profiling disabled. The newer 33.333 ms result does not
erase this earlier failure or establish that its underlying cause is fixed.
Neither result reintroduces the original automatic 203.199 ms full rebuild, but
neither justifies a claim that all presentation stalls are eliminated.

### Allocation and exact-equivalence evidence

The actual added Metal texture allocation is **318,988,288 bytes (304.211 MiB)**,
not the earlier approximately 231 MiB logical-texel estimate. Diagnostics expose
268,435,456 bytes for the spare density volume, 33,554,432 / 4,194,304 / 524,288
bytes for its reduction levels, 524,288 bytes for empty cells, 8,388,608 bytes for
cloud lighting, 3×1,048,576 bytes for air caches, and 221,184 bytes for the small
LUTs. These are `allocatedSize` values; logical dimensions do not predict the
actual private-texture allocation exactly.

The final recorded peak allocated Metal memory is 1,323,532,288 bytes, compared
with 1,001,865,216 bytes in the original climate artifact: an observed increase
of approximately **306.77 MiB**, below the 340 MiB proposal limit. The source
fingerprints differ, as expected for the implementation change, and these are
Metal allocation numbers rather than whole-process resident memory.

`liveVerification` in the final artifact reports a cold exact comparison of
published generation 5 at **sample time 19.899868 s, weather tick 9**. All
33,554,432 panorama RGBA components and all 8,192 irradiance components match
with **maximum error 0, mean error 0 and zero non-finite pixels**, under strict
zero tolerance. The explicit diagnostic took **293.885 ms GPU** and used a
16,896-byte result buffer. It is excluded from the live windows because it is a
separately requested blocking verification, not normal runtime work. The
in-progress next candidate was retired by that diagnostic; active publication
was preserved. This compares the same captured time/forcing, not a later paused
frame against an aged reprojected sky.

Soundstage's separate recorded live comparison in
`.build/sanctuary-native-20260912/coherent-stage-cache.json` also passed strict
zero error at source sample time 0.116667 s (generation 11, weather tick 0). Its
332.669 ms diagnostic is likewise separate from live performance. That older
Soundstage report lacked `skyCache` in its repeated workshop status snapshots,
so those samples are not used as evidence for the new scene-sun diagnostic.

Final game source digest:
`dd86fe7c801cc3b5e9176afb703489bf6cf989198795084cc31263e4bb89b535`.
Final shader digest:
`0634495bb25b251f2232ced0351529aa9f8d297052e2cce529a60c5ebbfda4c4`.
The JSON artifacts preserve the full workload/device state and capture paths.
Renderer scheduling, seven focused cache/light contract tests, native exact
comparison and these limited live windows are evidence for this repair; they
are not evidence that Sanctuary or the renderer has reached AAA completion.

## Pre-repair call chain and failure mode

1. `GameSession.draw(in:)` advances the game, calls
   `FrameComposer.game(_:time:look:wind:paused:)`, then calls
   `MetalRenderer.draw(_:elapsed:simulationMilliseconds:)`.
2. `FrameComposer.game` turns the selected `GameLighting.sky` fields, preset,
   exposure, wetness, time, and wind into `Uniforms`. Sanctuary's automatic
   climate changes named day/weather keys in
   `SanctuaryExperience.refreshClimatePresentation`; those keys alter several
   of the uniform fields used below.
3. `MetalRenderer.draw` creates one command buffer, immediately calls
   `Atmosphere.encode(_:_:paused:profile:)`, then encodes light state, shadow,
   scene, sky, post processing, presentation, and commits that same buffer.
   Its three-token `inFlight` semaphore only caps whole frames; it does not
   divide work inside this command buffer.
4. The pre-repair `Atmosphere.encode` derives `key` from `options.y`,
   `environment.x`, sun direction, and `sky`, and `densityKey` from coverage,
   density, and seed. `full` is true whenever `key` changed (or when a paused
   time changed). That makes a normal climate boundary take the synchronous
   branch.
5. The synchronous branch updates weather; when `densityKey` changed it
   dispatches all 1024×96×1024 cloud-density voxels, all three conservative
   max-reduction levels, and empty-cell certification. It then dispatches all
   64 cloud-lighting slices, all 512×256 air rows, all 8192×1024 sky rows, and
   the 64×32 irradiance integration. It copies the resulting sky/irradiance to
   the current/previous/pending publication textures. All of that precedes the
   visible scene in one command buffer.

The non-paused branch already proves the useful scheduling shape: it fixes a
`sampleTime`, advances weather once, and then dispatches cloud-lighting slices,
air rows, and sky rows over later frames into `pendingSky` and
`pendingIrradiance`. It swaps only after the complete pair exists. The defect
is that `changed` bypasses this path and writes the complete cache in the
presented frame. The existing density kernel supports a Y offset through
`u.environment.w`, but the Swift encoder currently dispatches the entire
density volume in one step; the reduction and empty-cell kernels need matching
slice offsets before a density-changing candidate can meet a per-frame budget.

`FrameComposer` and Sanctuary climate need no cache-specific branch. The
repair belongs in the reusable FieldEngine atmosphere cache.

## Original reusable active/candidate proposal

Use one immutable **active** cache and one **candidate** cache. A published
state consists of a previous/current sky+irradiance pair for the existing
cross-fade, together with the exact transmittance, multiple-scattering,
clear-sky/cloud-ambient, weather, density hierarchy, empty-cell, cloud-lighting
and air textures that produced it. A candidate owns a frozen copy of the same
inputs and outputs until it is complete. `noise` can remain shared because it
is immutable after its initial construction.

The additional candidate needs roughly 295 MiB of private texture storage when
its density differs: 192 MiB for 1024×96×1024 R16F density, about 27.75 MiB for
the reduction hierarchy and empty cells, 8 MiB for 256×64×256 R16F lighting,
64 MiB for 8192×1024 RGBA16F sky, plus small air/LUT/irradiance textures. This
is a deliberate memory-for-latency trade. Allocate it lazily, report it in
diagnostics, and reject the design if the measured resident allocation exceeds
the acceptance limit below. If only sun, preset, haze, or weather changes, the
candidate may reference the active density hierarchy read-only; only a changed
density signature allocates/rebuilds candidate density.

Use typed values rather than the current interpolated String key:

```swift
struct SkySignature: Hashable, Sendable {
  var medium: MediumSignature       // haze
  var density: DensitySignature     // coverage, density, cloud seed
  var lighting: LightingSignature   // preset, sun direction, time multiplier
  var weather: WeatherSignature     // saved weather seed/tick
}

struct SkyRequest: Sendable {
  let generation: UInt64
  let signature: SkySignature
  let sampleTime: Float
  let uniforms: Uniforms            // value captured once, never mutated
  let weather: MTLTexture           // fresh immutable upload for this request
}
```

`SkySignature` must contain only values that affect a cache texture. Exposure,
wetness, outdoor ambient floor, camera position, and camera orientation remain
per-frame scene values and must not start cache work. The weather signature is
the saved replay seed/tick, never wall-clock time.

The narrow API surface is:

```swift
enum SkyDelivery { case live, exactCapture }

func requestSky(_ request: SkyRequest, delivery: SkyDelivery)
func encodeCandidateStep(_ cb: MTLCommandBuffer, profile: GPUProfile?)
func encodeActiveFrame(_ uniforms: inout Uniforms)
func cacheStatus() -> SkyCacheStatus
```

`requestSky` is idempotent for an equal signature. For a live request it keeps
the active cache visible immediately and makes the newest request desired. It
does not call `waitUntilCompleted`, allocate textures, or perform a full compute
encode on the MTKView draw path. `encodeCandidateStep` issues at most one
budgeted unit: density Y slab, one reduction/empty-cell slab, one cloud-lighting
slice, 16 air rows, a small fixed number of sky rows, or irradiance. Each unit
uses the request's frozen uniforms and weather texture. Existing low-sun row
budgets remain a policy input, but must be reduced further if the measured frame
budget requires it.

The candidate's final compute pass is followed by a command-buffer completion
handler. Only a successful completion for the still-desired generation may
publish. Publication is a main-actor/reference swap before a later frame is
encoded: old current becomes `previous`, candidate becomes `current`, blend
time resets, and all fragment/light-state bindings for a frame are taken from
one local active-cache reference. No incomplete candidate texture is ever
bound by `MetalRenderer.draw`.

### Stale generation and cancellation rules

1. Increment `desiredGeneration` whenever a different signature arrives. Store
   its immutable request before scheduling any work.
2. A command buffer captures `(candidateID, generation, step)` and retains the
   candidate resources. Completion checks all three plus `status == .completed`.
   Error, stale generation, or a missing final step may only retire the
   candidate; they may not alter active textures or publication times.
3. A newer request stops encoding further old steps. Already committed GPU work
   is not cancellable and is allowed to drain. The next candidate starts only
   after every recorded old-step completion has released that staging set. This
   avoids overwriting a resource still referenced by an in-flight encoder.
4. If more changes arrive while draining, retain only the latest desired
   request. It is safe for intermediate climate states to be skipped; the
   visible active state persists until the latest complete candidate publishes.
5. Keep retired active caches strongly referenced until the frame-completion
   count associated with the renderer's three in-flight frames reaches zero.
   Then recycle them as candidate storage. Do not rely on a property swap alone
   to establish GPU lifetime.
6. `exactCapture` is explicit. A paused test/capture requests the same
   candidate and waits outside the ordinary draw loop for a ready generation;
   it must never force `full` work into the next presented frame. The capture
   records the published generation/signature. A live game is allowed to show
   the old completed cache while construction proceeds.

The implementation can use the existing serial `MTLCommandQueue`; command
buffer order gives the producer/consumer order needed for candidate steps.
There is no benefit in claiming a second queue creates extra GPU throughput.
The key safety properties are immutable captured inputs, no staging reuse before
completion, and atomic publication after completion. A separate `MTLSharedEvent`
is unnecessary for that initial same-queue implementation.

## Implementation scope

The intended implementation touches only reusable renderer code:

- `Engine/FieldEngine/Rendering/Atmosphere.swift`: typed signatures, cache-set
  ownership, staged candidate state machine, generation accounting, and cache
  diagnostics.
- `Engine/FieldEngine/Resources/Atmosphere.metal`: Y/slab offsets for density,
  reduction, and empty-cell stages; no lighting-model change.
- `Engine/FieldEngine/Rendering/MetalRenderer.swift`: bind one active cache per
  frame, tie retirement to command completion, and record active/candidate GPU
  timing separately.
- `Engine/FieldEngine/Rendering/PostProcess.swift`: consume the active cache
  reference passed by the renderer rather than reading mutable atmosphere
  properties independently.
- FieldEngine renderer tests: cache-generation, stale-publication, and exact
  paused-result coverage. Sanctuary remains a consumer and needs no special
  renderer switch.

The initial implementation should not change sky equations, SceneLook,
exposure, climate keys, or the transition blend duration. It schedules existing
work differently. The visual approximation remains the documented finite sky
cache and reprojection model.

## Experiment and acceptance plan

Use the existing `climate-native` saved-slot fixture and the exact fixed camera,
seed, render size, and climate sequence recorded by
`.build/sanctuary-native-20260912/climate-native-study.json`. Run the six-second
automatic boundary sequence at 60 Hz, once after warm-up and three times total.
Keep the ordinary game path; do not replace movement, weather, or cache work in
the harness.

Record these additional fields for every run: active generation/signature,
candidate generation/signature, candidate phase and slab/row count, candidate
GPU time, active-frame GPU time, publication count, stale-retirement count,
cache age, and allocated Metal bytes. Capture fixed views before the boundary,
while a candidate is building, immediately after publication, and after the
existing blend completes.

Accept the design only if all of the following hold on the same M4 workload:

- During the transition, presented-frame p95 is at most **20.0 ms** and maximum
  is at most **33.4 ms**. This removes the measured 199.999 ms hitch while
  allowing two display intervals for a bounded outlier.
- Active-frame GPU p95 is at most **16.7 ms** and its maximum at most **25 ms**;
  candidate-step timing is separately reported so a fast active frame cannot
  hide a queued cache stall.
- Peak allocated Metal bytes increase by no more than **340 MiB** over the
  recorded baseline for the same scene. A higher result requires a new memory
  design, not an undocumented budget increase.
- The completed candidate at fixed seed/time matches a cold exact rebuild of
  the same signature under existing renderer numerical tolerances. Paused
  capture waits for that generation and reports it; it never captures a partial
  cache.
- A rapid A → B → C climate request while A is building publishes C only. A and
  B have completed-or-retired records, no GPU errors, no use-after-free, and no
  change to active `skySnapshotTimes` before C's completion.
- The old no-change six-second live workload remains within its measured
  steady-state presentation behavior. Report p95 and maximum rather than
  averaging the one-time transition cost away.

If one density slab cannot fit the candidate-step budget, reduce slab height and
add the matching Metal offsets before relaxing the limits. If total cache build
latency becomes objectionable, report it as a separate age/quality trade-off;
do not reintroduce a synchronous rebuild to shorten that latency.

## Primary references and limits

- Apple, [Setting up a command structure](https://developer.apple.com/documentation/metal/setting-up-a-command-structure): command buffers submitted to one queue execute in submission order; resources/pipelines should be reused rather than created in a time-critical path.
- Apple, [Synchronizing CPU and GPU work](https://developer.apple.com/documentation/metal/synchronizing-cpu-and-gpu-work): immutable per-submission resources and bounded in-flight work are the appropriate default synchronization shape.
- Apple, [MTLCommandBuffer](https://developer.apple.com/documentation/metal/mtlcommandbuffer): completion handlers run after GPU execution and are the publication/retirement boundary used above.
- Sébastien Hillaire, [A Scalable and Production Ready Sky and Atmosphere Rendering Technique](https://sebh.github.io/publications/egsr2020.pdf), EGSR 2020: LUT-based sky work can be separated from final screen resolution and made scalable. It does not validate this renderer's cache dimensions, its cloud approximation, or these latency targets.

This proposal is a cache-lifetime and scheduling design, not a claim of async
compute overlap, full atmospheric transport, or AAA-quality completion. It
needs the measured acceptance run and visual capture comparison before any such
claim.
