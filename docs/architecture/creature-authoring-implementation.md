# Creature authoring implementation

This records the implementation of the [creature plan](creature-authoring-plan.md). Follow the [clay-first runbook](../creature-development-runbook.md) when using it. The system now supports an end-to-end source, compile, simulate, inspect, compare, and play loop. The original two study creatures remain development fixtures; implementation coverage is not a declaration that they meet the Elden Ring art bar.

## What is installed

| Area | Implemented behavior | Main entry points |
| --- | --- | --- |
| Anatomical source | Nonhuman regions, frames, landmarks, chart identities/revisions, anchors, attachments, masks, controls, review scenarios | `packages/model/src/creature.ts`, `creature-validation.ts` |
| Shape | Sweeps with cross sections/twist, thin closed patches, bounded local sculpt, local realization of hard-union sphere/ellipsoid features | `packages/compiler/src/creature.ts`, `creature-features.ts` |
| Correspondence | Anchors transport by chart; bounded projection exposes residual and ambiguity; stale topology rejects | Compiler chart/attachment helpers; authoring region/chart operations |
| Deformation | Anatomical influence exclusions, rigid attachments, sparse pose correctives, corrected normals, per-detail bindings | Compiler creature products; runtime `creature-deformation.ts` |
| Appearance | Anchored masks, correlated albedo/roughness variation, material families for skin, eye/wet surfaces, cloth, fibers, hard surfaces | Compiler groom/appearance modules; renderer creature material response |
| Groom | Deterministic roots/guides, flow/length/clump/curl/frizz/taper, scars and sparse growth, tufts and opaque ribbons, nested detail variants | `groom.ts`, `groom-creature.ts`, `creature-groom.ts` |
| Motion | Two-bone IK, iterative chain IK, planted contacts, ground orientation, bounded pelvis adjustment, pose layers, expressions, editable clip recipes, explicit retarget mapping | Runtime creature solver; authoring `creature-retarget.ts` |
| Dynamics | Fixed-step secondary chains, guide simulation, cloth pins/constraints/collisions, constrained Rapier articulation, impulses and authored recovery | Runtime creature dynamics modules and session |
| Lifecycle | Replay/checkpoint, save/restore, dormancy, reset/teleport, ownership ordering, immutable render extraction | `packages/runtime/src/session.ts` |
| Agent edits | Transactional local and proportion changes, preservation constraints, proposals, comparison, adoption, undo, bounded inverse source edits | `packages/authoring/CREATURES.md` |
| Long work | Resumable solver jobs, real event-loop yielding, progress, pause/resume/cancel, deadlines, bounded retention, stale-result protection | `creature-jobs.ts`, session and Studio discovery |
| Grounding | Units/spaces/support and editable source paths; dependency graph; current triangle/chart/material/groom/joint attribution with source and detail validation | `creature-provenance.ts`, runtime picking/inspection, Studio pixel tools |
| Review | Clay, rig overlay, silhouette, binding, normals, albedo, roughness, thickness, fiber direction and region channels; source-exact groom hiding; matched candidate pose contact sheets | Studio Creature panel and `window.wrela.creature` |
| Delivery | Typed cooked products, validation, worker transfers, generated materials, correspondence, variant bindings/correctives, source identities | `creature-cooked.ts`, cooked compiler |

The public operation reference and `discover()` are authoritative for request shapes. All edits produce ordinary source. Candidate preparation does not replace accepted work, and a solver's result does not automatically acquire visual approval.

## Where fields and compilation help

An anatomical chart is reused for geometry, scar placement, material masks, groom roots, attachments, binding, and local correction support. A proportion change can update those relationships through one public edit rather than unrelated mesh, texture, hair, and rig operations.

Small independent hard-union features compile through bounded parametric surface products instead of inheriting the body's uniform sample spacing. Smooth unions, subtraction, intersection, clipping, and shared ancestry retain their supported extraction path. Separate closed components preserve opaque exterior visibility; they are not a fused manifold or a signed-distance representation.

Preparation products have independent keys, byte limits, and immutable handoffs. Motion-only edits reuse shape, binding, appearance, and groom products. A mane-length change reuses body geometry and canonical root binding/corrective work. Per-detail products expand those root mappings without repeating the spatial bind for every hair vertex. Cache tests include mutation and worker-buffer detachment.

The same guide source can produce volumetric tufts or closed ribbons. Detail products retain stable roots and preserve directional support tips. Their cost and rest-guide bounds are measurable; appearance/coverage error remains explicitly unknown. The current body mesh does not gain an adaptive geometric LOD merely because the groom has detail variants.

Physics detail is separate from visible detail. Cloth uses a bounded source-chart simulation lattice with render followers. Single-character flat review stages specialize cloth/guide ground queries to their known plane, while foot support remains a general physical query; mixed scenes and worlds keep their general query path. This specialization has an explicit domain rather than a guessed plane in an arbitrary scene.

## Use and verification

Open **Creature** in Studio, choose an original study, and start with **Clay + skeleton**. Local sculpt, groom-guide, contact, material, and performance edits use the shared authoring session. Candidate comparison uses isolated hosts and a bounded renderer, preserving the accepted project and runtime. The study encounter is an explicit game exercise, not a generic generated encounter service.

```sh
bun run creature:review --output=output/creature-review.json
bun run creature:smoke --mode=clay --hide-groom --skeleton
bun run creature:smoke --biped --motion=walk --time=0.5
bun run creature:bench
bun tools/creature-studio-verification.ts
```

The review tool performs reproducible CPU experiments and records limitations. The smoke creates a finite no-water hardware capture. The Studio verifier exercises actual review/candidate/encounter/delivery paths. The CPU benchmark measures simulation and extraction for one and four actors of each body plan; it excludes renderer cost. Hardware tools retain source/environment manifests and use the shared GPU lease and watchdog.

Keep source stable during evidence runs or use an isolated source snapshot. A successful capture of an older snapshot does not validate later code. Comparison settings, simulation tick, source identity, and completeness must remain attached to the evidence.

## Acceptance that remains separate from implementation

- Clay anatomy and deformation must be reviewed at useful camera sizes and across the declared motion range. The first captures exposed lost fine features and dark double-multiplied groom color; those generated concrete fixes rather than an art approval.
- Skin and eye responses are bounded surface approximations. There is no general refractive cornea, full volumetric skin, or multilayer fiber scattering claim.
- Contacts and terrain adaptation do not establish convincing gait timing. The small study encounter uses a flat bounded arena; terrain-navigation behavior and a satisfying finished combat experience require further play review.
- Ragdoll recovery is an authored blend with bounded physical assistance, not a balance-aware get-up planner. Cloth reports residuals for conflicting constraints; it is not a high-end garment solver.
- The bounded inverse solver optimizes source region extents and contact-target coordinates. It does not optimize rendered silhouettes or measured foot-slip trajectories. Field-derived microdetail response research remains optional and unpromoted.
- Groom reduction preserves selected geometric support; its coverage, shadow, and temporal quality require matched moving captures. Body LOD error and general deformation approximation error remain unknown.
- Neither a small Apple GPU smoke nor a CPU-only frame budget establishes ordinary-hardware coverage. Release claims require named device/browser tests and full-scene timing distributions with the intended actor count.

The plan's final quality gates close only with retained visual, movement, play, and delivery evidence. See [hardware and performance verification](../research/creature-authoring-verification.md) and [experiment records](../research/creature-authoring-experiments.md) for measured results and unrun comparisons.

The subsequent [clay iteration phase](../research/creature-clay-iteration.md) adds oriented and curved sculpt fields, bounded local compiler refinement, pixel-grounded rest-field repair candidates, persistent review notebooks, matched clay atlases and an actual-runtime clip audit. It also fixes shared deformation-buffer ownership and character-space contact targets under physical root motion. Its evidence supersedes the earlier test counts for this integrated source; artistic acceptance remains separate.
