# Hero authoring and continuous toolkit development

The authoring loop now has two complementary paths: refine existing semantic source, or construct a new rigid hero asset from a blank workspace. Both use the same proposals, source protection, rendered review, publication and portable evidence.

## Implemented

- A component graph of hollow revolved shells, rings, rods, beams and plates; named parents, material slots, sockets and existing hinge/slider articulation. The new shell primitive retains a real cavity and smooths curved normals while preserving sharp meridian transitions. It is bounded and rejects intersecting profiles.
- Blockout, construction, surface and production review stages. Production generates beauty, clay, grazing, detail, motion and gameplay-distance evidence. A retained design can advance as a new branch. References and a quality brief travel with its evidence.
- Targeted grain-origin, corner-wear, botanical branch-gap and material-response repairs. Branches can combine selected material decisions. Pinned source properties are checked before persistence or rendering.
- A bounded warm renderer/cache, shared by Studio, a lazy MCP host, and a local authenticated session transport. Source, resolution, camera, diagnostic mode and pose tick participate in capture identity. Renderer resources are explicitly closed.
- Reusable construction recipes with reference rebinding, immutable implementation revisions, source-bound provenance and fresh review on each instantiation. The initial portable scope is rigid objects, vegetation and their materials.
- A searchable toolkit with generic built-in discovery; explicit gap records; recipe/operation/compiler classification; engineering-time fields; source-bound cross-family trials; and separate technical validation versus recorded artistic acceptance. Unknown engineering time is `null`.
- Thin premultiplied glass transmission with Fresnel reflection, material-range distance sorting, and no opaque glass shadows. It lets agents build readable enclosures. This is an approximation: no thick-glass refraction, volumetric absorption or caustics, and intersecting transparency/water remains limited.
- Capture PNG encoding in a short-lived worker, with browser fallbacks and explicit cleanup. Per-view evidence now separates scene preparation from capture time.
- Less regular timber grain and knots, plus per-member grain origins that do not change geometry or connections.

## Workflow

The machine entry points are `wrela_hero`, `wrela_iterate`, `wrela_toolkit`, and the existing `wrela_job`, `wrela_context`, `wrela_study`, `wrela_finish`. CLI transport uses the same request implementation:

```sh
bun tools/authoring-service.ts /tmp/authoring-session.json
bun tools/authoring-service.ts invoke /tmp/authoring-session.json invocation.json
bun tools/authoring-service.ts close /tmp/authoring-session.json
```

An invocation has `{workspace, work, action, input}`. `hero` takes a current `expectedKey`, study `id`, and up to four explicit `designs`. A design has its own `id`, `name`, `family`, `intent`, `stage`, `age`, `seed`, and `components`. `iterate` takes a new proposal ID, optional base proposal, and exactly one revised design, repair, operation batch, or material combination. A failed pin cannot be bypassed by a favorable render.

After visual acceptance, `toolkit` action `remember-creation` stores a versioned recipe. `instantiate` returns operations with new document IDs and preserved internal part identities. For a capability gap, record the attempted composition and failed evidence before choosing the smallest useful extension. Register new implementations as experimental; `trial` binds actual usage to reviewed candidate source; promotion to validated requires two distinct asset families. Approved status additionally requires recorded visual decisions and previews. These claims remain attributable to their reviewers; the registry is not an automatic art judge.

Gap attachments are copied into managed evidence and survive removal of the request directory and a backup/restore cycle. The toolkit is local and explicitly versioned; it does not automatically synthesize compiler code, approve its own art, or publish a global asset marketplace. A failed composition can justify a small recipe without immediately growing the compiler.

## Verification and measured results

Scripted tool latency, fresh-agent wall time, toolkit transfer, and visual judgement are separate measurements. Exact model tokens and dollar cost are unavailable; neither is estimated from wall time. A passing source constraint is not an AAA art certification.

### Independent agents

| Task | Workflow | Full elapsed | Authoring tool time | First candidate image | Independent checks |
| --- | --- | ---: | ---: | ---: | ---: |
| Weathered timber | Select/refine three generated alternatives | 78.500 s | 2.335 s | 1.532 s | 16 passed |
| Exposed pine | Select/refine three generated alternatives | 70.151 s | 4.754 s | 2.824 s | 9 passed |
| Expedition lantern | Blank workspace; create and revise | 257.596 s | 2.859 s | 137.976 s | 2 passed |
| Hanging signal bell | Blank workspace; create and revise | 273.916 s | 2.544 s | 127.808 s | 2 passed |

Each task used a fresh agent without parent design guidance and completed with no engine edits or failed authoring-tool calls. Wood/pine used two authoring calls each, with generation beginning inside the attempt clock before agent selection. Each hero used three: create, revise, finish. Ordinary shell/file/image-inspection work is outside the authoring-call count. There is one agent observation per domain; these are not latency distributions.

The lantern agent composed 39 components, producing 18,416 triangles at 0.524 m tall. Its revision connected the glass seat and cap supports. The bell agent composed 25 components, producing 12,000 triangles at 0.605 m tall. Its revision adjusted bronze response and added a lower detail camera that exposes the cavity and clapper. Both inspected six-view production sheets. Art acceptance remains pending.

The previous comparable engine-started confirmations took 53.971 s for wood and 55.017 s for pine. **These new complete agent runs were slower**, despite lower tool time (previously 10.030/5.381 s). Most elapsed time sits outside measured authoring calls: model/host scheduling, reading, design, image inspection, shell work and orchestration are not individually instrumented. It would be incorrect to label all of that time model reasoning or claim an orders-of-magnitude end-to-end gain.

[Timber](hero-authoring-loop-2026-09-23/wood-contact-sheet.png) · [Pine](hero-authoring-loop-2026-09-23/vegetation-contact-sheet.png) · [Lantern](hero-authoring-loop-2026-09-23/lantern-contact-sheet.png) · [Bell](hero-authoring-loop-2026-09-23/bell-contact-sheet.png) · [Machine-readable results](hero-authoring-loop-2026-09-23/results.json)

### Final isolated tool timings

Five paired cold/warm inputs per domain, 40 samples total, all passing. Each pair has identical candidate source. Wood/pine generate three alternatives with three views each; the hero fixtures generate one known design with six views. The browser's warm startup took 0.620 s once. Cold means a fresh browser, not a purged driver or disk cache. Work setup and browser shutdown are excluded; cold startup is shown separately. Both tasks paused heavy CPU/GPU work for this sweep.

| Scripted study | Cold tool median | Cold including startup | Warm tool median | Warm p95 / slowest of five |
| --- | ---: | ---: | ---: | ---: |
| Timber | 1.801 s | 2.441 s | **1.200 s** | 1.834 s |
| Pine | 4.151 s | 4.780 s | **2.978 s** | 3.587 s |
| Lantern, six views | 1.349 s | 1.974 s | **0.793 s** | 0.811 s |
| Bell, six views | 1.194 s | 1.807 s | **0.647 s** | 0.667 s |

These are rendered, source-checked, saved tool outputs. They exclude agent design and image-inspection time and do not establish production tail latency from only five observations. The immediate objective of subsecond warm hero review is demonstrated for these fixtures. Subsecond complete agent authorship, cheap model usage and AAA art acceptance are not demonstrated.

Reproduce from the final frozen source with a new output directory:

```sh
cd output/hero-authoring-finalized/source
bun tools/hero-authoring-latency.ts ../latency-new ../../hero-authoring-release/study 5
```

[Full timing distributions and samples](hero-authoring-loop-2026-09-23/latency.json)

### Actual toolkit transfer

The delivered lantern and bell bundles were restored into one registry workspace. Both source-bound passing reviews qualified `hollow-revolved-shell` as **validated** across two families. `thin-glass` has one lantern trial and remains **experimental**. Attempts to promote either to artistically approved were rejected because neither has the required recorded acceptance. Engineering cost is unknown, not inferred from the complete authoring attempt times stored with the trials.

The lantern's actual thin-frame critique also became a portable open gap record. Its next experiment is an arc-path recipe using existing sweeps before considering another compiler primitive. Its recorded engineering time is zero because that follow-up has not started. The existing shell implementation's engineering time remains null.

The first registry integration run exposed a search bug: an added ranking field was fed back into the strict entry schema. That was fixed and covered by discovery/register/search CAS-key tests. A separate test now deletes the original diagnostic attachment directory before restoring a gap record, proving its evidence is owned and portable.

[Registry transfer evidence](hero-authoring-loop-2026-09-23/toolkit-transfer.json)

### Verification and source identity

The integrated checkpoint passed **1,077 tests across 233 files**, general/browser TypeScript, dependency boundaries and the Studio/player/worker/offline build. The final isolated authoring build passed another 24 focused tests, both TypeScript checks and production build. Studio's real browser flow exercised construction, six production views, pinned repair, material combination, construction-recipe reuse, export/restore of 60 artifacts, and persistence of three sessions and one toolkit entry across reload.

Warm/cold review pixels match exactly across changed source, domains, resolution and diagnostic modes. The cache remains bounded, budget stopping works, and worker, offscreen fallback and DOM fallback encoding preserve decoded pixels. Separate bundle restore/re-export checks preserved source keys and all 22 wood, 22 pine, 27 lantern and 27 bell image files.

Fresh agents used frozen source `a346cb9bc8816d16b007af6b3ee6d7671cbae1e118fdec97796edf175ea74c37` under `output/hero-authoring-release/source`. The later corrections cover registry search/attachment retention, unbiased grain-origin variation and the final blank-baseline PNG path. The final isolated engine is `840f073f2ce42389f58572f11f2474d4cac092243e292cb71041ff32d20dada7` under `output/hero-authoring-finalized/source`: it starts from the original benchmark snapshot and applies only seven authoring files, excluding subsequent concurrent lighting changes. The broad integration checkpoint is `8b821c717e58a98889726312a73294ea84baa268fb93b8f08750bbb64374183f`.

Replaying all four delivered sources on the final engine preserved every candidate source key and **all 18 candidate PNGs byte-for-byte**. This is output-equivalence verification, not four additional fresh-agent runs. The final tool-latency sweep uses the corrected engine; the agent wall times above remain the actual earlier observations.

[Studio browser report](hero-authoring-loop-2026-09-23/studio-browser.json) · [Studio panel](hero-authoring-loop-2026-09-23/studio-panel.png) · [Cache and encoder checks](hero-authoring-loop-2026-09-23/cache-check.json) · [Delivery replay](hero-authoring-loop-2026-09-23/delivery-replay.json)

### Performance debugging retained as evidence

The first five-pair sweep exposed a regression rather than validating the warm-session assumption: lantern median tool time rose from 2.38 s cold to 6.88 s warm. Profiling separated about 40 ms of scene preparation and tens of milliseconds of GPU readback from repeated approximately 1,010 ms PNG encodes. Moving an offscreen canvas onto the same main thread did not resolve the stall. Encoding was then moved into an owned worker; the final isolated warm lantern median is 0.793 s.

After moving frame and contact-sheet encoding, a later sweep still measured bell at 1.163 s cold versus 1.533 s warm. Per-view evidence showed fast GPU capture; the remaining stall was encoding the blank-workspace baseline placeholders. That path now shares the worker encoder too. The first sweep after this correction overlapped another task's full CPU test suite; it is explicitly excluded from headline performance results and retained at `output/hero-authoring-finalized/latency/measurement-context.json`. The final sweep runs with both tasks' CPU-heavy and GPU work paused.

The failed latency-harness request included resolution fields unsupported by the existing study schema; it failed before generation. That harness error and the slow renderer samples remain in their original output directories. No fresh-agent attempts were overwritten.

### Interpretation of quality and transfer

Both hero tasks start with one neutral material and no geometry. Fresh agents may use the generic schema and API guide, but may not read complete starting designs, fixtures or previous attempts. The separate scripted latency sweep deliberately uses known designs: it measures engine execution, not original design work.

Lantern and bell test two families sharing a hollow rotational construction primitive. Success there is useful transfer evidence, not proof that the same small grammar covers sculpted faces, cloth, arbitrary boolean cuts or every hero prop. The current glass is a thin-surface approximation. Timber remains visibly procedural up close, and the pine retains repeated branch tiers and dark foliage. None of these runs establishes AAA art quality, a cheaper dollar cost, or an Unreal/Blender performance ranking.
