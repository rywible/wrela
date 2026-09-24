# Construction and shared environmental history

The alpine source now uses one deterministic environmental history to drive physical stone damage, material treatments, plant vigor, and deposited rubble. The generated results remain ordinary material, assembly, vegetation, terrain and world documents. Rendering, collision, authoring transactions, undo and saving continue to consume those documents through their existing interfaces.

`packages/model/src/surface-history.ts` owns the shared source and evaluator:

```ts
const history = alpineSurfaceHistory({
  seed: 73,
  ageYears: 140,
  prevailingWetness: 0.18,
  damage: 0.28,
});
const signals = sampleSurfaceHistory(history, {
  position: [0, 0.12, 0],
  normal: [0, 0, -1],
  shelter: 0.25,
});
```

The source stores origin, up, rain direction, ground/water elevations, contact rise, runoff spacing, age, rain exposure, prevailing wetness and damage. Distances use metres; ground and water heights are signed distances along normalized up relative to origin. An explicit contact value supports samples whose world placement will subsequently be grounded on terrain. The evaluator returns bounded contact, exposure, runoff, age, wetness, dirt, damage, moss and debris signals. This is an authored cause model, not an erosion or hydrology simulation. It evaluates source history rather than advancing live weather state.

`surface-history-material.ts` turns those signals into a bounded contact/sheltered/runoff/exposed/broken material palette. Derived materials retain the underlying substance and receive coherent wetting, deposits and colonization. Mineral surfaces opt into 4–7 mm physical stone relief through the existing multiscale relief pipeline. Fine appearance and near geometry retain the compiler's explicit budget/error limitations. Palette classes approximate within-part spatial history; they do not claim a continuous surface wetness simulation.

`construction-recipe.ts` supplies deterministic cut-stone profiles, dimensioned coursed walls, wedge arches, and loose rubble. Convex corner losses remove material inside a measured envelope; they never expand a doorway. Stable identifiers and seeds select variation. Assembly budgets remain bounded to 128 parts. A final history pass resolves articulated part positions before assigning palette classes and wear, so attached timber and hardware inherit their source hierarchy.

The primary `createArchitectureLookdev({ history, seed })` retains the working hinge, gate descendants, standing clearance and editable sources. It adds damaged stone profiles, damp contact materials and footing/collapse fragments. `createArchitectureLookdev({ variant: "wayside", history, seed })` exercises the same construction helpers with a roofless room, narrow arched window, ruined return wall, bench and rubble. Its standing entrance is separately reserved and checked against the compiled mesh.

`world-construction.ts` creates bounded debris assemblies and stable plant-community candidates. Shared curved-path distance excludes fragments and plants from the walking core. Each debris patch is one grounded world instance containing editable parts, keeping placement costs bounded. `alpine-habitat.ts` retains the existing plant source IDs while deriving placement exclusions and vigor from the same history. Grounded patch assemblies follow one terrain anchor; they are suitable for local deposits, not arbitrary large-area terrain conforming meshes.

`lookdev-scene.ts` wires this history into the primary gateway, trail, habitat and four deposition/contact patches. `alpine-composition.ts` provides `createAlpineCompositionEdits(project, "river-bend")`, returning plain `document.set` descriptors. It changes the structure, its shared material appearances, landmark position and orientation, path layout, terrain corridor and terrace, review camera and plant exclusions. The model package does not import authoring. Consumers apply the descriptors through their existing authoring transaction interface.

## Evidence

Run `bun tools/alpine-construction-review.ts --revision=history-v1` for source JSON and a machine-readable report. The initial CPU review records:

| Measure | Primary gateway | River-bend shelter |
| --- | ---: | ---: |
| Editable assembly parts | 117 | 91 |
| Source assembly triangles | 11,768 | 11,072 |
| Realized world instances | 166 | 168 |
| Structural clearance warnings | 0 | 0 |
| Maximum sampled analytic route grade | 8.04% | 5.06% |

The alternate composition applies in one normal authoring transaction and survives undo, redo and JSON save/parse with matching source identities. The CPU report includes each primary part's history signals and class, source recipes, material appearances, route samples and structural diagnostics. Tests also check transformed history frames, exact source preservation, a curved walking-core exclusion, physical material relief and separate functional openings.

These results establish source integration, structural clearance and sampled analytic grading. They do not certify runtime triangle-collision traversal, render residency, GPU cost or AAA appearance. Root's serialized lookdev captures must judge the actual generated surfaces, contact, planting density, silhouette and second composition against the preserved baseline.
