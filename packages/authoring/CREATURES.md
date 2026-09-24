# Creature authoring operations

Creature source stays on an ordinary character document. `AuthoringSession.apply` and `preview` accept the same semantic operations in Studio and the headless authoring tool. `discover()` returns their machine-readable schemas, limits, units, and method descriptions. Unknown creature fields are rejected, including nested guide and mask fields.

All examples below use the currently inspected session revision. A headless session starts at revision zero; its `workspaceKey` independently protects publication against every workspace writer.

## Anatomy and connected edits

Initialize with `creature.initialize {target, source}`. Upsert one stable identity with `creature.region`, `landmark`, `chart`, `anchor`, `attachment`, `sculpt`, `appearance`, `groom`, `influence`, `corrective`, `contact`, `ik`, `secondary`, `expression`, `cloth`, or `review`; each has `{target,value}` and the corresponding model schema. `creature.pelvis {target,value}` replaces bounded root/contact adaptation, with `value:null` to clear. `creature.articulation {target,value}` replaces the articulated body/constraint source; `value:null` clears it. `creature.remove {target,domain,id}` removes one entry only if the resulting transaction still satisfies all references.

`creature.proportion {target,region,scale:[x,y,z],preserve?:regionIds,propagate?:"region"|"descendants"}` coordinates editable field shapes, region charts, anatomical frames, landmarks, rig positions, joint motion displacement, masks, guide positions, local correctives, character-space contacts, and IK targets. `creature.move` takes `translation` instead of `scale`.

Chart anchors keep their authored coordinates and offsets. Compiler-bound groom roots follow the chart. Mounted components retain their authored dimensions and attachment offsets; the compiler relocates them using their anchors. Anchored appearance masks retain their local mark offsets. Groom length/width, world-space contact targets, and attachment clearance remain physical authored values. Spherical sculpt/mask support scales for uniform edits; nonuniform edits transport its center while retaining its physical radius. This is an intentional art-control policy, not a promise that every detail undergoes an exact affine deformation.

Hard `preserve` constraints protect the declared source for those regions and reject known cross-system dependencies: moved anchors driving protected cloth pins or mounted components, and moved joint ancestors driving protected rig/influence/corrective controls. Selected joints whose rest and authored control transforms remain identical do not block the edit; a width-only change can preserve an axial neck. Changes to radius-based multi-joint binding envelopes still reject where they could change protected weights. Rigid or single-influence bindings remain weight one. Shared node/joint controls across selected and unselected regions, unsupported shears, transformed legacy parent frames, and cross-scope IK chains reject the complete batch. Straight cardinal sweeps support nonuniform elliptical sections. Nonuniform curved sweeps require explicit section authoring. The tool never silently chooses a nearest limb, relaxes preservation, or approximates an unrepresentable source transform.

A chart geometry replacement requires either `correspondence:"preserve"` with unchanged revision and compatible ordering/orientation, or an increased revision and explicit anchor/groom/cloth repairs in the same batch. Control-point reordering is not an ordinary sculpt edit. Local sculpt operations preserve chart parameter semantics.

Landmark-fitted clothing retains four `fittingLandmarks`; landmark anchors retain their `landmark` reference. Use `creature.landmark` to move one: the same transaction updates dependent patch corners and pin/mount anchor coordinates while retaining correspondence. Folded or reversed fittings reject atomically, and low-level edits that bypass propagation reject as stale source. `releaseCreatureGarmentFit(character,id)` freezes an existing garment fit as independent chart/anchor source without moving it. New `mountCreatureAttachment` operations use `placement:"surface"`, which seats actual primitive support at the requested tangent-plane clearance before adding the explicit offset. The compiler respects field-parent rotation and keeps mounted dimensions unchanged. This local fitting is not a guarantee of whole-body collision clearance.

## Inspect, select, explain

- `inspectCreature(target, region?)` returns anatomical source and connected controls.
- `selectCreature(target, point, radius?)` selects region support boxes in character space. Results explicitly say `exactSurface:false`; they are not rendered pixel hits.
- `explainCreature(target, region)` traces declared geometry, correspondence, appearance, grooming, attachment, deformation, and performance dependencies. It does not invent measured sensitivities or pass costs.

## Precise local form and repairs

`creature.sculpt` supports oriented ellipsoidal `support:{radii,rotation}`, a piecewise-linear `path` of 2–32 region-local points, and `mode:"push"|"flatten"|"inflate"`. Radii and displacement are meters; Euler rotation is radians. Without explicit support it retains the original spherical radius. Push uses a fixed displacement, flatten approaches the plane through the closest stroke point with displacement as its normal and physical movement cap, and inflate moves radially from that point. Mirroring reflects the entire support, path, and displacement. Layers evaluate additively at the original point. Optional `nodeIds` restrict the layer to named field surfaces or geometry charts in the region; use this to protect eyes and teeth from a cheek edit.

`detail:{maxEdgeLength,passes}` requests sampling near the sculpt support before displacement. The compiler conformingly splits shared mesh edges, preserves draw groups, and warns when its pass/vertex budget cannot meet the request. Explicit sweep/patch charts increase bounded tessellation. This recovers narrow sculpt features between existing vertices; it does not resample the original implicit surface or establish a global geometric error bound. Sculpt introspection exposes support and source paths. Proportion edits transport stroke paths; an oriented ellipsoid that cannot represent a requested affine edit exactly rejects rather than silently changing its shape.

`repairCreature({id,target,sourceKey,expectedRevision,region,nodeId?,handles,protectedPoints?,support,maxDisplacement?,detail?,intent})` fits an isolated candidate of ordinary push strokes. Each handle is `{id,position,target,tolerance}` in character-rest meters; a protected point is `{id,position,tolerance}`. Positions refer to the currently sculpted rest surface. The tool inverts existing local sculpt where reliable, fits compact controls, reports their measured weights and residuals, and rejects stale source, incompatible protection, unreliable inversion, excessive displacement, and excessively steep fitted fields. A region containing surface-scoped layers requires an explicit `nodeId`.

The fit is a rest-field constraint, not a guarantee about compiled silhouettes, appearance displacement, skinning, or artistic quality. Inspect the compiled candidate and its posed motion before adopting. `window.wrela.creature.repairPixel({id,pixel,restDisplacement,support,protectPixels?,tolerance?,detail?,intent})` grounds actual displayed body pixels, uses their rest points and exact source ownership, and prepares this candidate. It rejects ambiguous multi-source hits, groom, mounted components, and pins on another surface. Displacement is explicitly a rest-space vector, not a screen-space drag.

Persistent evidence uses `creatureNotebookSchema`, `appendCreatureObservation`, `assessCreatureObservation`, and `inspectCreatureNotebook`. Observations retain source identity, anatomical region, intent, protected features, capture/camera/motion/tick and priority. Assessments append evidence and a reason. An assessment only applies to its exact character source; a subsequent edit reopens review without erasing history. Compiler/runtime revisions belong in the attached capture manifest and must also be reviewed after code changes.

```sh
bun run creature:notebook output/warden-notebook.json init ash-warden
bun run creature:notebook output/warden-notebook.json inspect
bun run creature:notebook output/warden-notebook.json observe observation-request.json
bun run creature:notebook output/warden-notebook.json assess assessment-request.json
```

Mutation requests contain the last inspected notebook `expectedKey` plus `observation` or `assessment`. Writes are locked and atomically replaced; a stale key rejects. Notes live beside the project so adding evidence does not invalidate the geometry it describes.

## Alternatives and bounded solving

`proposeCandidate({id,batch,budget?})` validates edits without replacing accepted source. Candidate state includes the original dependency preconditions, exact source revision/key, proposed changes, and provenance from the batch. `compareCandidates(left,right?)` compares ordinary source against a shared baseline. Visual acceptance still requires matched review scenarios.

`adoptCandidate(id, metadata?)` uses the original transaction preconditions. Stale work rejects; identical retries return the original receipt. `cancelCandidate(id)` rejects unaccepted work, `releaseCandidate(id)` frees retained alternatives, and ordinary undo/revert handles accepted edits. Candidate notifications do not change the source revision.

`solveCreature` produces an isolated candidate using bounded finite-difference coordinate Gauss–Newton. It supports region-scale controls and contact-target coordinates, with region-extent and contact-target source objectives. Parameter ranges must contain current source. Hard preserved regions remain protected. The result reports actual residuals, active bounds, insensitive controls, rejected constraints, work counts, and convergence/budget status. It does **not** claim to minimize rendered silhouette error, evaluated foot slip, or artistic quality. Those require runtime/compiler evidence outside this package.

The synchronous API remains available, capped at 16 controls, 32 objectives, 64 iterations, and 1,024 evaluations. Interactive clients should use `startCreatureSolveJob({request,deadlineMs?,evaluationsPerSlice?})`. This returns immediately, retains an immutable source snapshot, and drives the same resumable solver through real event-loop yields between bounded source evaluations. Each slice performs at most 1–8 evaluations (default 1) and stops scheduling more work after 8 ms. One source evaluation remains atomic; cancellation or deadline expiry is checked before and after it.

Use `inspectCreatureSolveJob(id)`, `listCreatureSolveJobs()`, and `creatureSolveJobUsage()` for progress. `awaitCreatureSolveJob(id)` resolves to a terminal snapshot. Pause/resume retain the exact continuation; cancel discards unaccepted work. Deadlines include queued and paused time. Terminal states are `completed`, `cancelled`, `deadline-exceeded`, `stale`, or `failed`; nonterminal states are `queued`, `running`, and `paused`. A completed computation reports its own convergence status separately and prepares an ordinary reviewable candidate without adopting it. If source changed during work, the job retains its batch and residuals as `stale`, with no automatically rebased candidate.

At most four jobs run cooperatively, sixteen are retained, each source snapshot is capped at an estimated 4 MiB of serialized UTF-16 source, and live snapshots total at most 32 MiB by that measure (not a JavaScript heap measurement). `releaseCreatureSolveJob(id)` frees a terminal record while reserving its identity against duplicate execution. Identical retained requests are idempotent even after source changes; differing reuse rejects. Job IDs and continuations are process-local. Replacing a project cancels pending work. A source edit after completion still causes candidate adoption to reject through the original preconditions.

## Headless workflows

```
bun run author <workspace> creature-inspect <character> [region]
bun run author <workspace> creature-explain <character> <region>
bun run author <workspace> creature-select <request.json>
bun run author <workspace> creature-propose <request.json>
bun run author <workspace> creature-solve <request.json>
bun run author <workspace> creature-solve-async <request.json>
bun run author <workspace> creature-repair <request.json>
bun run author <workspace> creature-retarget <request.json>
bun run author <workspace> creature-motion <request.json>
```

Every JSON request contains `workspaceKey`. Select adds `target,point,radius?`; propose adds `id,batch`; solve adds `request` matching `creatureSolveRequestSchema`; solve-async adds `job` matching `creatureSolveJobRequestSchema` and waits for its terminal result; retarget adds `source,target,retarget` matching `creatureRetargetSchema`; motion adds `target,clip` matching `creatureClipSourceSchema`.

Repair adds `request` matching `creatureRepairRequestSchema`; it returns a transferable candidate batch and measured fit without publishing.

Retargeting requires named joint mappings, explicit translation scale, and explicit contact-space conversion where relevant. Named pose libraries plus timed pose references compile to ordinary editable motion keys. Retargeting and motion creation return proposed source rather than publishing it.

CLI candidates are ephemeral. The returned `{workspaceKey,batch,...}` can be saved as a transferable proposal, inspected with `preview`, and published with `apply`. Candidate IDs do not persist across CLI processes. Only `apply` writes the workspace, and its existing source-key guard rejects stale publication.
