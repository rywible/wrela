# Agent authoring transfer: timber and vegetation

The shared authoring workflow now supports semantic material editing and botanical growth editing. Both domains use the same bounded search, persistence, hardware review, proposal selection, publication and portable evidence path. The initial fresh agents completed wood in 97.717 seconds and vegetation in 71.656 seconds, with no failed authoring calls. A matched-light comparison then exposed insufficient silvering in the initial wood adapter. That result was retained; the adapter now desaturates the underlying fibres as exposure and maturity increase, and fresh confirmation agents completed wood in 108.011 seconds and vegetation in 75.191 seconds.

## Implementation

- `packages/authoring/src/authoring-domain.ts`: two compact domain adapters. Shared controls are exposure, moisture, maturity and variation, with explicit bounds and domain-specific interpretations. Timber creates member-scoped materials, grain/end grain, silvering and ground staining while protecting geometry and shared materials. Vegetation edits crown density, asymmetry, tropism, droop, stiffness and existing conifer controls while preserving species, seed, materials, dimensions, pruning and manual edits. These are authored approximations, not calibrated environmental simulation.
- `authoring-study.ts`: deterministic bounded candidate search, immutable source and optimistic concurrency, retained failures, source-bound review packets, visual quality criteria/reference metadata and one comparison gallery. A candidate can pass structural checks without being artistically accepted.
- `review-stage.ts`, `packet.ts`, `study.ts`: shared fixed neutral/grazing fixtures and detail views. Missing stages fail instead of silently substituting lighting. Identical fixtures illuminate baseline and candidate. Fixed daylight captures reject blank, dark or constant images. GPU evaluation shares one browser across the study.
- `study-recipe.ts`: approved conditions become reusable semantic recipes only after an explicit accepted review decision. Promotion verifies that the conditions reproduce the accepted operations. Transfer replans for the new target and requires new review; it never transfers artistic acceptance.
- `tools/authoring-agent.ts`: native stdio MCP tools with inline image output, plus a CLI-compatible invocation bridge. The fresh agents used the bridge and host image display; the native endpoint was not installed into the running host's tool list. Native tool handlers, inline PNG output and stdio discovery/error handling were separately tested.
- Studio uses these same APIs through **Explore reviewed alternatives** and **Remember accepted study**. Subject selection follows the work's preserved target. CLI help/task context expose bounded controls, units, quality criteria, examples and next actions.
- Generic geometry constraints measure compiled extents and triangle budgets, including expanded foliage instances. They are not GPU-performance estimates.

## Initial fresh-agent results

| Task | Wall time | First candidate image | Authoring calls | Failed calls | Tool time | Independent checks |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Prior wood weathering | 313.063 s | 84.660 s | 7 | 1 | 6.600 s | 16 |
| Initial shared wood study | 97.717 s | 40.413 s | 3 | 0 | 6.085 s | 16 |
| Initial shared vegetation study | 71.656 s | 44.331 s | 2 | 0 | 8.591 s | 9 |

The wood agent inspected six candidates across two studies. The vegetation agent inspected three candidates and a matched baseline comparison. Both published source, passed independent checks, and exported portable handoffs without engine edits. The wood observation is 3.20× faster than the prior run; it is a directional observation from individual trials, not an isolated causal estimate or a statistically established speedup.

[Initial wood](authoring-domain-transfer-2026-09-23/initial-wood-contact-sheet.png), [initial vegetation](authoring-domain-transfer-2026-09-23/initial-vegetation-contact-sheet.png), and [prior weathered wood versus the initial shortcut under identical lighting](authoring-domain-transfer-2026-09-23/previous-vs-initial-wood.png) preserve the visual evidence. The last comparison shows why speed alone was not sufficient: the initial shortcut was too warm and patchy compared with the previous weathered result.

## Fresh confirmation after the wood correction

Both new fresh agents completed against the corrected frozen source, with no engine edits or failed authoring calls:

| Task | Wall time | First candidate image | Authoring calls | Failed calls | Tool time | Independent checks |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Wood | 108.011 s | 37.731 s | 3 | 0 | 6.329 s | 16 |
| Vegetation | 75.191 s | 37.471 s | 2 | 0 | 8.647 s | 9 |

The wood agent again compared six alternatives, this time selecting a less uniformly pale result with stronger ground moisture staining. The vegetation agent selected among three alternatives, then inspected a matched baseline sheet. Corrected wood took **108 seconds versus the prior 313 seconds (2.90× faster)**. Vegetation took **75 seconds**, close to the initial 72-second observation. No repeated-run distribution or independent art score is claimed.

[Final wood comparison](authoring-domain-transfer-2026-09-23/final-wood-contact-sheet.png) and [final vegetation comparison](authoring-domain-transfer-2026-09-23/final-vegetation-contact-sheet.png). Baseline on the left, selected candidate on the right. The wood baseline is the accepted geometric revision, not the prior weathered final asset. The corrected wood now has more coherent silvering; regular grain and limited end-face detail remain. The pine keeps its identity and develops more readable crown gaps, but the silhouette changes are restrained and fine foliage remains dark.

Between initial and confirmation snapshots, the other active task also changed radiance-lighting implementation/tests. The source fingerprints preserve those changes; the visual and timing difference cannot be attributed exclusively to the wood recipe. The vegetation adapter itself was unchanged.

## Transfer and regression checks

Scripted hardware tests exercise the gate and a braced fence, plus a pine and a birch, through the same native tool handlers. All generate visible review images, pass source/geometry constraints and publish portable handoffs. These scripted timings do not count as fresh-agent results.

[Gate gallery](authoring-domain-transfer-2026-09-23/timber-primary-gallery.png), [fence gallery](authoring-domain-transfer-2026-09-23/timber-heldout-gallery.png), [pine gallery](authoring-domain-transfer-2026-09-23/vegetation-primary-gallery.png), [birch gallery](authoring-domain-transfer-2026-09-23/vegetation-heldout-gallery.png).

A real browser test checks both domains in IndexedDB: proposal/review persistence, accepted-recipe storage, replay on the second shape/species, portable gallery relocation, reload of four sessions and two recipes, rejection of black review images, and generating another three alternatives through the actual Studio button. The acceptance decisions in this storage fixture are explicitly test fixtures, not artistic judgements. [Studio check](authoring-domain-transfer-2026-09-23/studio-check.json), [panel](authoring-domain-transfer-2026-09-23/studio-panel.png).

The broad integration run passed 1,033 tests. After the silvering correction, focused authoring tests passed 13 tests/107 assertions. TypeScript, browser TypeScript, dependency boundaries and the Studio/player/worker/offline build passed. The original full run caught a concurrent lighting buffer-layout assertion; its owning task corrected it before the clean broad run. No changes were made here to the lighting renderer or its layout. The only rendering change in this task adds low-frequency drift to the existing procedural wood grain.

Failed experiments remain in `output/authoring-transfer-validation` (unlit black fixture images) and the initial benchmark directory. The black captures led to explicit sun initialization and the blank-image regression guard. Two browser harness mistakes—mutating an immutable backup fixture and returning a DOM node from a boolean wait—were corrected before the passing browser run; neither was a hidden agent failure.

## What this supports and what it does not

The common workflow transferred to a geometry/growth domain without a second search/review/publication implementation. Transfer also worked across two shapes and two species. The result supports the shared-infrastructure/domain-adapter design for these cases. It does not establish universal authoring, AAA art quality, or superiority over Unreal or Blender.

Visual shortcomings remain: wood still has overly smooth grain and limited construction damage; vegetation has dark fine foliage, fairly regular branch tiers and subtle exposure asymmetry. There is no independent blind artistic acceptance or approved reference board in this experiment. Reference retention and explicit review targets exist; real art approval must still supply the quality bar. The adapters generate useful starting points, and broader domains need their own rules and evidence.

Most agent wall time remains outside the measured CLI spans. That includes startup, reasoning, shell preparation and image inspection; it is not an instrumented model-time measurement. Exact tokens, model version and intervention counts are unavailable, so formal benchmark qualification remains incomplete. The shared host can also contain other work; GPU work uses a cooperative lease, but CPU/model scheduling is not laboratory-isolated.

## Reproduction and source identity

Initial benchmark: `output/authoring-domain-benchmark/study`, source fingerprint `56c2f0c0c47adfdf852c3d3ba030fac9cc42f20d0b8509e41f657777986a21ee`.

Corrected adapter/confirmation: `output/authoring-domain-confirmation`, source fingerprint `2772aa1c59565b6efe718a9d92370e0f2fa24a33668185c53a5ebdcb07817fa9`.

Broad-test snapshot: `output/authoring-domain-final-validation/source`, fingerprint `7b00d986e6a6236923c8a3a2904ec1225a1a32bb03ddf1033934149641f43ed2`. Studio UI/build snapshot: `output/authoring-domain-studio-final/source`, fingerprint `3356df445ce8d1a75496b7bd088134a42b351bc1f9165ce89c733a2703764815`. Final differences after the broad run are the tested Studio subject default, browser harness improvements, timber silvering and its regression test; concurrent lighting work is separately frozen in each snapshot.

Run `bun tools/authoring-transfer-check.ts <output>` for the scripted hardware/native-handler checks, `bun tools/authoring-study-browser-check.ts` for the browser workflow, and `bun tools/authoring-transfer-benchmark.ts <new-suite> <previous-suite>` to preregister the two fresh-agent tasks against a frozen engine. The preparation command uses the previous revision source and geometric checks for wood; the vegetation task is new. Fresh-agent runs use the existing benchmark runner and atomic submission bridge, with 600-second/16,000-token declared budgets and seed 104729. The original weathering run had a baseline-lighting fallback that this implementation forbids, so the review requirements are stricter in this experiment.
