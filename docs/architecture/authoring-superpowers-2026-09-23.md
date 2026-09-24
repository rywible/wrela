# Faster domain authoring with stronger visual construction

The authoring engine now supports one bounded job from a brief, with curated starting points, retained alternatives, fixed-light images, and a ready publication request. Source publication and artistic acceptance remain separate. Timber and vegetation use the same orchestration; each has its own construction rules.

## Implemented

- `authoringJob` / `wrela_job` / `author <workspace> work job <id> request.json` resolve the subject, choose a curated target, apply optional condition overrides, generate and review up to four alternatives, and return the current source-safe finish request. No discovery call is required.
- Six explicit starting points cover silvered, sheltered and damp timber; exposed, sheltered and dry crowns. Each records its intent, conditions and visual failure modes. They are **curated and unapproved**, not a fabricated library of independently accepted art.
- Studio exposes the same job, starting-point selector and 15/30/60-second exploration budget. More detailed condition controls remain available.
- One GPU renderer serves the entire study. Fixed baseline captures are cached by complete source, target, camera, rig, tick and resolution. The renderer and cache are disposed at the end, including failure paths. No persistent background process holds the shared GPU lease.
- Every gallery includes the baseline and every alternative under the same neutral and grazing lights. Failed or budget-skipped alternatives remain recorded. The first review completes even if the budget expires; further alternatives stop starting. Reported overruns include work/session overhead; this is not a hard real-time guarantee.
- Wood now has a continuous solid growth field across side faces, bevels and end grain, varying growth-band widths, localized knot collars that bend fibres, filtered longitudinal checks, restrained silvering and ground-height deposition. Geometry adds highlight-bearing end bevels and deterministic corner erosion while preserving the broad-face envelope, authored path, sockets and attachments.
- Pine authoring now coordinates directional crown asymmetry, branch-correlated gaps, live-shoot retention, needle dimensions and current/older cohort contrast. Shared near geometry and filtered distant shoots derive their colour from the same canonical recipe. Species, seed, dimensions, material ownership, pruning and manual edits stay protected. Optional structure controls at zero reproduce the old geometry.
- Compiler product versions invalidate obsolete geometry and botanical products. Tests cover deterministic replay, closed worn meshes, unchanged envelope, preserved branch attachment and edited twigs, and colour changes without changed needle geometry.
- Benchmark startup has a compact task brief and executable job request. The runner records image availability, completed review availability and finished delivery at 15, 30 and 60 seconds. These are observable milestones, not artistic scores.

## What the automatic recommendation means

The tool ranks constraint-passing alternatives by distance to the requested authored conditions. Its visual score is explicitly null. An agent sees the whole comparison and makes the visual choice before publishing. Arbitrary prose is used for transparent starting-point retrieval and review direction; it is not represented as a calibrated material or botanical solver.

Accepted semantic recipes from earlier work remain portable and require fresh review on transfer. These six new defaults have not been approved by a human. Training or calibrating an automatic art critic still requires an actual reference set and independent judgements.

## Reproduction

```sh
# Native MCP endpoint, for a host that registers it:
bun tools/authoring-agent.ts --mcp

# The equivalent host-independent bridge:
bun tools/authoring-agent.ts invocation.json
# {"workspace":"/absolute/workspace","work":"asset","action":"job","input":{"budgetSeconds":30}}

bun tools/authoring-review-cache-check.ts
bun tools/authoring-exemplar-check.ts output/authoring-exemplars-new
bun tools/authoring-study-browser-check.ts
```

The native MCP handler returns images inline. The fresh trials use the equivalent CLI bridge because this conversation does not have that endpoint registered; the host displays the returned image paths before finishing.

## Evidence and limits

Frozen benchmark engine: `output/authoring-superpower-final/source`, fingerprint `40f14d8b368a0d195a62bc737a28eaafd8f0e992b945642b17432d4f771d9c0a`. Earlier visual iterations are retained under `output/authoring-superpower-lookdev`, `-v2`, and `-v3`.

The warm-render check found zero differing pixel channels against fresh rendering for both timber and pine, and verified cache reuse and budget stopping. The real Studio browser check exercised generation through the UI, persistence across reload, portable evidence restoration, recipe transfer and blank-image rejection.

The six native exemplar jobs completed and exported portable source/evidence in 2.16–3.18 seconds each on this machine with one candidate per job. These are scripted checks with a warm driver cache, not fresh agent completion times or general performance guarantees. Additional candidate captures reused the baseline and took approximately 0.15 seconds in the focused three-candidate run. Complete jobs include browser launch, source validation, image encoding, storage and export.

Source-based constraints do not prove AAA quality. These are stronger editable starting assets. Important remaining art work includes richer joint-specific construction and damage, species-specific botanical detail beyond the current pine grammar, motion/LOD review in game context, and independent reference-based acceptance. The pine's retained material palette and the shared lighting system also affect its dark canopy appearance.

The wood approach is informed by the structural idea of random-access solid growth fields in [Procedural Wood Textures](https://arxiv.org/abs/1511.04224), but its knot approximation is our own and does not reproduce a full anatomical model. Crown organisation takes inspiration from the importance of coherent growth in [Self-organizing Tree Models for Image Synthesis](https://algorithmicbotany.org/papers/selforg.sig2009.html); the implementation is a bounded authored grammar, not that paper's light-competition solver.

## Initial fresh runs and dispatch correction

The first two new fresh agents completed wood in **135.056 seconds** and vegetation in **90.538 seconds**, with two successful authoring calls each and no failed authoring calls. Independent checks passed (16 wood, 9 vegetation); both source fingerprints remained unchanged during the attempts. This was slower end to end than the previous 108.011/75.191-second observations, despite reducing tool runtime to 3.031/5.222 seconds. We retain these results rather than calling the faster tool runtime an agent speedup.

Wood's first candidate image arrived after 63.051 seconds; vegetation's after 36.932 seconds. No output was delivered within 60 seconds. Unattributed time includes host/model scheduling, command preparation, image review and orchestration. Exact model/token counts remain unavailable.

The vegetation agent also exposed a CLI ergonomics defect: `author --help` was treated as a workspace. Help is now read-only and includes job and portability commands. The portability checker now accepts actual exported bundles as well as work records. Independent restore/re-export checks preserved every source key and all 22 retained image files for both initial attempts (50 wood artifacts, 44 vegetation artifacts).

Dispatch now starts a known task's authoring job immediately and gives the fresh agent its reviewed gallery and a single explicit selection command. The timer starts **before** workspace preparation and generation. The engine does not select or publish; the independent agent still visually compares, may refine, and chooses the delivered proposal. `--finish-job <dispatch.json> <proposal> [benchmark-request.json]` retains source CAS and exact review validation. It cannot turn a forged passing flag into an accepted proposal.

The confirmation uses `dispatch: engine-started` and a separate suite. This changes task dispatch, so it is reported separately from the fully agent-started runs. It is not uncounted precomputation. No warmup outside the reported clock is attributed to the independent agent.

[Timber starting point](authoring-superpowers-2026-09-23/silvered-trail-timber.png) · [Pine starting point](authoring-superpowers-2026-09-23/exposed-mature-pine.png) · [Transfer to a fence](authoring-superpowers-2026-09-23/damp-old-timber.png) · [Transfer to a birch](authoring-superpowers-2026-09-23/dry-open-crown.png)

[Warm-render equivalence measurements](authoring-superpowers-2026-09-23/cache-check.json) · [Studio browser report](authoring-superpowers-2026-09-23/study-browser-report.json) · [Studio panel](authoring-superpowers-2026-09-23/study-panel.png)

The frozen confirmation source (`output/authoring-dispatch-ready/source`, `b50b0d0f2af4f38ced19f6dc4eb8c02f0cd02ec478ee89baa82b0fe8ae4a385e`) passed 1,050 tests across 228 files, general and browser TypeScript checks, and the Studio/player/compiler-worker/offline build. This snapshot excludes unrelated research files by the repository's source-manifest policy; the earlier root run included them and passed 1,080 tests before the final dispatch/CLI additions. Targeted tests separately cover dispatch forgery, stale-source rejection, read-only help and cohort appearance.

## Fresh confirmation with generation at dispatch

| Task | Prior confirmation | Initial agent-started run | Engine-started confirmation | First image | Complete tool runtime | Independent checks |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Wood | 108.011 s | 135.056 s | **53.971 s** | 8.203 s | 10.030 s | 16 passed |
| Vegetation | 75.191 s | 90.538 s | **55.017 s** | 2.714 s | 5.381 s | 9 passed |

Both confirmations had two successful authoring operations (the engine-dispatched job plus the agent's explicit finish), no failed authoring calls, no discovery calls, no engine edits, reviewed alternatives available within 15 seconds, and completed delivery within 60 seconds. Neither delivered within 30 seconds. Both elapsed totals include workspace setup, generation, parent/agent dispatch, visual selection, publication, export and independent constraint verification.

The new fresh agents independently selected the same conditions as the initial pair: nominal silvered timber and the more exposed/open pine. Their final angle, detail and grazing PNGs are byte-identical to the respective initial run's delivered images. Dispatch improved observed completion time without changing those visual outputs. Against the immediately previous confirmation this is about 2.00× faster for wood and 1.37× for vegetation; it is not an orders-of-magnitude end-to-end gain, and single runs do not establish a latency distribution.

[Wood comparison](authoring-superpowers-2026-09-23/wood-contact-sheet.png) · [Pine comparison](authoring-superpowers-2026-09-23/vegetation-contact-sheet.png) · [Wood run record](authoring-superpowers-2026-09-23/wood-attempt.json) · [Pine run record](authoring-superpowers-2026-09-23/vegetation-attempt.json)

Independent portability checks restored and re-exported all 50 wood / 44 vegetation artifacts and preserved every source key and all 22 image files in each bundle. The agent's quality comments remain useful: regular grain and restrained ground staining on wood; dense upper foliage and dark interior branches on pine. These are not AAA acceptance certificates.

No exact token price, model version or human-intervention instrumentation is available. Formal independent-art benchmark qualification remains incomplete. GPU work used the shared lease, and the concurrent lighting task's snapshots are preserved in the engine fingerprints. Driver cache and ordinary host/model scheduling affect timings; no universal performance guarantee or Unreal/Blender ranking follows from these samples.


## Matched comparison against the previous delivered assets

[Previous/current timber](authoring-superpowers-2026-09-23/previous-vs-current-wood.png) · [Previous/current pine](authoring-superpowers-2026-09-23/previous-vs-current-vegetation.png)

These sheets compare the previous confirmation's delivered source (left, labelled baseline) with the final new agent's delivered source (right, labelled candidate). Both are rendered by the final frozen engine at 1280×960, with identical cameras and neutral/grazing lights. This isolates authored source differences; both sides receive the new shader and compiler, so it is not an old-renderer/new-renderer comparison. Full captures and packets are retained under `output/authoring-dispatch-ready/matched-comparison/{wood,vegetation}`.

The timber has brighter silvering, stronger grain readability and clearer highlight-bearing bevels. Its close-up grain remains visibly regular and its joint-specific wear remains restrained. The pine has fuller, more coherent foliage masses and crown asymmetry, but still shows regular branch tiers and dark interiors. These improvements are visible; fully AAA visual quality has not been established or achieved across these assets.
