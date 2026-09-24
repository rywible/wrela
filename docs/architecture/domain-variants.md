# Agent authoring alternatives

`AuthoringSession.proposeDomainVariants` expands named, bounded edit recipes into the ordinary operations already used by Studio. It uses the existing generic candidate registry, source revision guards, document validation, source comparison, adoption, persistence and undo. There is no second transaction system. `proposeCandidates` adds atomic preparation of several generic alternatives: a later invalid proposal cannot leave earlier candidates retained.

Use the returned candidate ID for inspection and adoption. Its bounded readable prefix and hash derive from the request/variant pair, so full-length source IDs and delimiter-ambiguous pairs remain distinct. Display labels may truncate; complete request and variant identities remain in the review result.

Each request supplies an identity, the inspected `expectedRevision`, and one to eight variants. Each variant supplies its own identity, a recipe and optional `preserve` source paths. All alternatives start from the same accepted project; alternatives are not applied cumulatively. The candidate identity is `<request id>-<variant id>` and must satisfy the ordinary identity limit. Adopting one candidate advances the source revision and makes its siblings stale. A repeated candidate identity may only describe the same batch.

```ts
const result = session.proposeDomainVariants({
  id: "open-canopy",
  expectedRevision: session.getSnapshot().revision,
  variants: [0.35, 0.6, 0.82].map((density, index) => ({
    id: `option-${index + 1}`,
    recipe: { kind: "vegetation.canopy", target: "alpine-pine", density },
    preserve: [{ target: "alpine-pine", path: ["height"] }],
  })),
});
session.compareCandidates(result.variants[0].candidate.id, result.variants[1].candidate.id);
session.adoptCandidate(result.variants[1].candidate.id);
```

`discover().domainVariants` exposes the complete JSON request schema, recipe descriptions and currently usable target identities. Optional domain authoring must exist first; a canopy recipe cannot silently initialize a different botanical generator. Missing protected paths reject, rather than appearing preserved because both sides are absent. No-op variants reject so empty edits cannot inflate productivity counts.

| Recipe | Edits | Retained source / limits |
| --- | --- | --- |
| `vegetation.canopy` | Botanical canopy density | Whole-plant dimensions, seed, woody growth, pruning, branch edits and leaf dimensions remain identical. Projected coverage is not certified. |
| `assembly.weather` | Wear amount on all or selected existing parts | Every part dimension, connection, joint and clearance stays identical. Existing wear is optical coloration, not new erosion geometry. |
| `geology.erosion` | Talus erosion strength | Requires an authored protected corridor. Corridor profiles, formations, base terrain and local interventions remain unchanged. Runtime grade and obstacle review remain necessary. |
| `material.history` | Weathering, wetness, dirt and damage | Relief, detail, material family, optical response and layers stay identical. Every user of the shared material is affected. |
| `creature.landmarkSpan` | Uniform region/descendant fit between two landmarks | Reuses coherent creature proportion edits and protected-region checks, including dependent fittings. One scalar span does not solve an anatomical shape. |
| `performance.retime` | Clip duration | Existing retiming scales body/face keys, markers, contacts and alignments together; other clips and rest anatomy stay unchanged. New-speed gait/contact quality needs runtime review. |
| `world.population` | One population's density | Seed, spacing, admission rules, routes, placements and explicit exceptions remain identical. Newly admitted plants still require gameplay review. |
| `environment.grade` | Exposure and tint | Applies consistently to the base and all existing sequence states; retains weather values, key times and interpolation. It does not change atmospheric transport. |

These are reusable editing recipes over existing definitions. Versioned creation templates remain the model's existing `RecipeDefinition` / `instantiateRecipe` system; these helpers do not introduce another serialized asset-template format.

## Review contract and costs

Every variant returns its ordinary candidate, passing source-preservation checks, bounded leaf-level source changes with an explicit truncation flag, operation count and changed-document UTF-8 bytes. The review contract names subjects, requested views, pending measurements, baseline/candidate source keys and relevant limitations. Its initial visual status is always `unreviewed`. Source equality is explicitly labeled `authored-source`; it never asserts equivalent silhouettes, traversal, normals, radiance or motion.

`sourcePreparationMs` measures bounded source planning/validation, not complete visual update latency. Compiler time, first visible frame, GPU p95, resident memory and temporal error remain pending until supplied by a renderer or measurement harness. Candidate adoption does not imply artistic approval.

The headless CLI supports `variants request.json` and domain-independent `propose request.json`. Requests wrap the session request in `{workspaceKey, request}`; generic proposals use `{workspaceKey, id, batch}`. `variants` returns `proposals`, each already shaped as `{workspaceKey, batch, report}` for existing `preview` and `apply`. Candidate IDs are ephemeral across CLI processes. Only `apply` publishes; the workspace key prevents a stale alternative from overwriting another writer. Saved accepted documents contain ordinary source and retain normal reopening behavior.

## Matched production review

`bun tools/domain-variant-lookdev.ts` captures all eight domains. `--domain=vegetation,materials` selects a subset; `--small` uses 640×480. Run through a frozen source snapshot while other agents are editing. The tool uses the shared hardware-browser lease and existing `createLookdevFixture`, `BrowserSceneHost` and production WebGPU renderer. No image substitute grants approval.

Each domain has one baseline and three parameter values declared before capture, with the same three cameras/times per alternative. The output retains the initial project, request, source checks/diffs, all three projects, matched images, review manifest and gallery. Every browser case repeats the source request and ordinary candidate adoption before preparing a new host and renderer. Measurements separate source work, adoption, new-host preparation, first complete frame, and all-frame/readback duration. The request-to-first-complete interval excludes browser startup, transport and human review. In-process compiler caches may already be warm; this is not a claim of a cold application startup or sustained gameplay frame rate. Every frame retains adapter measurements, host memory, upload attempts and optional indirect-lighting evidence.

Performance frames use matched wall-clock samples of the retimed clip, intentionally showing different clip phases. Other fixtures compare the same camera and simulation time. Three screenshots cannot establish temporal stability; foliage trajectories and continuous play remain separate gates. Parameter hold-outs demonstrate repeatable source generation, not generalization to unrelated species, construction systems or game scenes. Token cost, manual interventions and artistic accepted counts remain `null` until measured, never zero by assumption.

Focused tests exercise three alternatives in each domain, whole-request rollback, stale siblings, protected-source failure, ordinary adoption/save/reopen/undo/redo, generic batch conflicts, CLI publication, and predeclared production studies. GPU completion and visual verdicts must be recorded separately after actual captures.
