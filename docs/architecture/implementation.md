# Architecture implementation

This work implements the September 21 architecture review in the current shared checkout. The independent mathematical research under `tools/field-research/` is outside this change.

The representative experience is **Winter valley exploration**, selected by the user: guide the bunny through a short exploration challenge, restore trail markers, and return home. Its acceptance includes movement, a following camera, interactions, sound, save/resume, a clear outcome, and automated action/state inspection.

## Work and ownership

| Area | Implementation | Acceptance |
| --- | --- | --- |
| Authoring | Incremental document state, dependency preconditions, retry-safe transactions, semantic change history | Disjoint edits commit safely; dependent edits conflict; large-project edits avoid whole-project copies |
| Storage | Cross-process workspace publication, independent recovery drafts | Concurrent writers cannot both publish from one base; divergent drafts remain recoverable |
| Runtime | Authoritative clock, bounded live controls/replay, regional actors, compiled tracks, collision/query events | Long input sessions, separated characters, delayed visuals, coherent time and save/replay |
| World | Critical collision readiness, stable population budgets, persistent moved entities | Delayed visual work does not block prepared collision; dense rules remain distributed |
| Compiler | Fidelity diagnostics/IR, product dependencies, recipes/migrations, cooked artifacts | Authored feature failures are visible; unchanged geometry reused; artifacts round-trip |
| Renderer | Completeness, shared draw ranges, visibility, stable material domains, HDR/AA/display, quality profiles, attributed timing | Incomplete captures fail; material groups share vertices; hardware captures pass |
| Game | Trusted game module and Winter valley exploration | Playable interaction loop with camera/sound/save/resume and inspectable actions |
| Studio | Capture/recovery integration, clear play entry, focused preview services | Human/agent UI shares current contracts and exposes degraded/error states |
| Verification | Source/build manifests, typed fixtures, workload/performance gates, CI | Evidence identifies exact source and scenario; correctness cannot pass by omitting content |

## Target policy

The reference desktop scenario targets 1080p at 60 Hz; a lower-power profile reduces render and residency cost explicitly. Local timing gates are engineering baselines, not claims that untested devices meet the target. Hardware support requires evidence from each supported device/browser family. Reports distinguish frame-callback pacing from display presentation and CPU submission from GPU execution.

## Integration

Run focused tests for each changed contract, then strict types/import boundaries/formatting, the complete CPU suite, production build, hardware browser verification, playable-slice automation, and repeated performance workloads. Save pass/failure manifests and images under `output/`. Existing math research files are neither reformatted nor rewritten by this work.

## Implemented contracts

1. **Playable experience and simulation.** Winter valley has semantic movement and interactions, a following camera, three lantern restorations, warmth, success/failure, audio, pause, touch/keyboard controls and portable saves. Trusted game rules live separately from Player presentation. Fixed ticks are authoritative; snapshot extraction does not advance time. Continuous controls use a bounded, tick-indexed replay window. Controller movement can retain visual root motion without adding it to physical displacement.
2. **Critical world readiness.** Collision products publish without waiting for optional scenery. Compatible overlapping patches and edge dependencies publish together. Source, residency and collision revisions are distinct. Active actors share physical interests; distant actors keep explicit dormant state. Generation uses byte admission, cancellation, deadlines and a bounded quarantine for unresponsive jobs.
3. **Truthful rendering.** Every requested identity has a rendered, culled, uploading or rejected outcome. Strict capture rejects refused allocations. Identity metadata comes from the actual identity pass; each channel retains its own completeness record. Picking uses selected render geometry.
4. **Independent writers.** Publication locks span the final comparison and atomic pointer replacement across processes. Browser recovery uses project/writer/base identities and retains divergent drafts. The same transactions are available in the browser and through the headless authoring command.
5. **Compiler analysis and product reuse.** Field IR records transforms, provenance, conservative or explicitly unknown bounds, dependencies and evaluation cost. Exact-safe pruning reduces the separated-union test's node evaluations by more than tenfold. Geometry, binding, motion and material assignment have distinct keys and bounded caches. Unresolved small features produce actionable diagnostics; explicit strict fidelity requirements fail instead of quietly losing content.
6. **Visible work and memory.** Material ranges share vertex resources. Camera and shadow culling account for deformation. Generated vegetation has a genuinely cheaper distant realization with projected-size hysteresis. Frame measurements distinguish submission, attributed GPU work, resource ownership and quality choices. CPU artifact accounting deduplicates installed/cache ownership.
7. **Procedural composition.** Saturated placement balances rules and spatial regions rather than filling the first row. Stable identities survive traversal. Once a procedural occurrence is meaningfully moved, its retained definition/state no longer depends on eligibility at its original site.
8. **Image pipeline.** Scene-linear HDR, explicit display conversion, anti-aliasing, stable procedural coordinate domains, bounded material layers, specialized shader variants and stabilized shadow projections support a quiet winter palette with warm lantern accents. Diagnostic channels bypass display filtering. Low/balanced/high profiles coordinate resolution, shadow cost, distant detail and upload work.
9. **Incremental agent authoring.** Transactions structurally share unchanged documents, incrementally validate affected references, cache source hashes and bound changed-document history. Explicit read/write revisions allow independent proposals. Retry IDs, actor/intent, semantic diffs, preview and targeted revert make changes reviewable.
10. **Reusable meaning and physical realization.** Parameterized recipes produce editable definitions with seed, overrides and provenance. Refresh is explicit and refuses accidental overwriting of edited realizations. Static triangle and compound collision preserve traversable shapes; filtered queries/contact events support game rules. Compiled animation tracks, loop displacement and bounded motion events avoid repeated per-frame indexing.
11. **Delivery and compatibility.** Build/export includes cooked geometry with source/product/compiler/format identities. Release bundles verify the actual compiler fingerprint. Missing/incompatible products rebuild only through an explicit fallback. Source migrations preserve inputs; trusted save migration plans name exact artifact and motion remaps. Unknown future formats and unknown changes reject explicitly.
12. **Verification.** Typed browser fixtures cover the identified failures and the complete game. Reports bind to actual working-tree source, browser/hardware, resolution and scenario; changed source invalidates a run. Performance uses uniquely attributed GPU samples, p95/p99 pacing, hitch/completeness/resource gates, repeated views, traversal and actor density. CPU/build CI and an opt-in named hardware runner retain evidence.

## Deliberate boundaries

- General field and skinned-character simplification is not enabled. Uniform extraction cannot yet certify a surface-error bound, and automatically choosing a cheaper hero mesh could erase semantic features. Current vegetation detail is explicitly a projected-size heuristic with unknown geometric error; it is reported as such. The actual valley must pass its budgets while hero geometry remains intact.
- Field fidelity metadata is not a proof that every generic implicit surface is a signed distance or has a certified Hausdorff error. Diagnostics and strict rejection are part of the contract until a measured, validated extraction method replaces that uncertainty.
- Active/dormant policy uses bounded radii and interests. Replay rolls its oldest accessible baseline when capacity fills; this is not unlimited archival recording. Procedural terrain is still generated while playing; finite authored geometry is cooked.
- Resource reports count owned GPU buffers/textures and deduplicated CPU artifacts. Driver swapchain allocation, deferred driver frees, JavaScript object overhead and unpublished worker products are not exact process-memory measurements.
- Browser frame-callback intervals are not display-present timestamps. Metal/Chromium evidence is local evidence. Other device/browser combinations need their own hardware runs before release support is claimed; the self-hosted CI runner must be provisioned separately.
- This is a working production foundation and complete small game loop, not a declaration that the reference art and game feel have reached the studio's final AAA bar. Playing, art direction and targeted content iteration remain the product judge.

## Final evidence — September 21, 2026

Verified source: `2ebcd35113e52e5a2171940070572eb983f43e71a37b29c8f4920d211bc6558c`. All three final browser runs finished with the same source fingerprint they started with. Independent field/transport research is excluded from that implementation fingerprint and has not been edited.

- **CPU:** 233 tests passed, zero failures. Both TypeScript configurations and package boundaries passed. Scoped formatting/lint passed across 133 implementation files. The whole-repository check remains red only on `tools/transport-research/**` formatting/lint from the independent math task.
- **Integration:** 19 Studio checks, 12 real IndexedDB checks, and six game/delivery checks passed. The exported game restored all three lanterns and returned home through the real physics/input path. Native keyboard playback, pause, profile switching, local/portable restore, failure/restart, mobile layout, offline loading and device-loss recovery passed.
- **Delivery:** the exported reference reached its first complete frame in **511 ms**, consuming three cooked finite definitions and compiling none from source. Terrain remained streamed as designed.
- **Authoring:** the 200-definition, 3.73 MB workload accepted edits at **17.21 ms p95**, versus the review's 280–375 ms measurements. 199 unchanged definitions retained shared identity. This measures acceptance, not a large-project edit-to-visible latency claim.
- **Images:** equivalent 256 m world-material rebasing differed by at most one RGB value; an uncompensated control differed substantially. Budget refusal rejected capture with the missing surface identity. Fresh beauty captures confirm restored close-subject contact shadows.

Named local hardware: **Apple M4, 16 GiB memory, macOS 25.6 kernel, Chromium 153 / Metal**. Each performance profile ran two 240-frame stationary samples, 480 travel frames covering 288 metres, and 240 frames with thirteen characters. All timed frames were complete; there were no rejected or uploading omissions, and GPU timing covered every sampled frame.

| Profile | Output / internal scene | Worst CPU p95 | Worst GPU p95 | Worst callback p99 | Peak owned GPU bytes |
| --- | --- | ---: | ---: | ---: | ---: |
| Balanced | 1920×1080 / 1920×1080 | 10.60 ms | 13.70 ms | 19.90 ms | 148,637,492 |
| Performance | 1280×720 / 960×540 | 10.70 ms | 8.26 ms | 17.90 ms | 24,973,384 |

Peak deduplicated CPU artifacts were 5,076,242 bytes in both profiles. These are local engineering results, not evidence for slower untested machines or true display-present timing.

Reproducible local reports:

- `output/browser-1790016879577-44183/verification.json`, source/environment/run manifests, Studio/game/diagnostic screenshots and the tested static build.
- `output/browser-1790016943815-44540/performance.json` — balanced profile.
- `output/browser-1790016968118-44535/performance.json` — Performance profile.
- `output/architecture-tests.log`, `output/architecture-check.log`, `output/architecture-scope-check.log`, `output/architecture-authoring.json`.
- `dist/build-manifest.json`, `dist/source-manifest.json`, `dist/compiler-identity.json` and the matching cooked products.
