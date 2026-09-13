# Sanctuary regional ground appearance — source candidate

2026-09-12. Source candidate only; no candidate build or renderer validation performed by this author. Root owns serial build, Soundstage and native review. No baseline approval.

## Observed problem

Actual pre-change PNGs inspected:
- `.build/sanctuary-native-20260912/ground-creek-native/runtime/captures/ground-creek-soft-bank-106aec7f.png`: footprint meshes are legible, but surrounding terrain is nearly uniform olive green.
- `.build/sanctuary-native-20260912/ground-creek-native/runtime/captures/ground-creek-eye-view-54fa2d9b.png`: sparse grass and willow context expose the lack of readable substrate variation.

## Source contract

`BiomeGroundAppearance.sample` blends existing continuous biome weights, grade from the existing vertex normal, smooth route proximity and signed natural/saved-water coverage. These are appearance affinities, not simulated soil or hydrology. Its RGB is actual authored albedo, retained by near and distant terrain. Material kind 14 modulates that RGB; it does not replace the biome map. Existing terrain positions, normals, tessellation, collision, feature IDs and save data are unchanged.

`ground-creek`, `ground-alpine` and `ground-meadow` compile patches from the same Terrain and appearance sampler as regional terrain. Each default patch is 12 m across with 1,089 vertices and 2,048 triangles. Offset controls allow the patch to move through the real source. Vertices retain world metre coordinates, and an instance translation centers the studio representation; local-coordinate detail therefore has the same phase as the scene at that location. The patches are substrate-only and do not invent water surfaces or vegetation.

The material uses four derivative-filtered noise bands: approximately 4.55 m, 0.59 m, 0.22 m and 0.059 m. Bump amplitudes are millimetres, not geometry displacement. The shared filter fades each unresolved band to its mean. Dry/damp roughness spans approximately .96 to .80; the damp shader hint comes from the authored darker bank albedo, so it is an appearance cue rather than an independent physical measurement. Global surface controls still apply. No global exposure, cabin recipe, water recipe, renderer ABI or shared shader files changed.

All CPU classification occurs at mesh compilation using the existing sampled height and normal. Five analytic natural water fields are evaluated with the existing height supplied; there are no additional Terrain.height calls, upwind queries or per-frame terrain work. Optional garden coverage uses its established saved-patch union; only signedCoverage is consumed, not the computed bed/surface. Existing mesh caches and stream transaction lifetimes are retained. Shader cost is bounded to four filtered noise bands on kind-14 fragments; cost remains to be measured by root.

## Root replay and acceptance

After the normal source build and renderer shader checks, select the project and catalog:

```sh
./scripts/stagectl project sanctuary
./scripts/stagectl catalog
./scripts/stagectl subject ground-creek
./scripts/stagectl rig softbox
./scripts/stagectl view quarter
./scripts/stagectl pause true
./scripts/stagectl saveStudy ground-creek-source-candidate.json
./scripts/stagectl captureReview
./scripts/stagectl styleBoard
```

Repeat subject selection/study capture for `ground-alpine` and `ground-meadow`. Review actual front/quarter/back/above PNGs under noon, golden, sunset, afterglow, overcast, rain, indoor and softbox, with matched wet/dry settings and the shared look. Inspect metre-scale patch boundaries and a close ground angle; then repeat the original creek native route and examine footprints, water-bank transition and distant terrain. No new baseline should be accepted automatically.

Acceptance remains pending: readable restrained earth/stone variation rather than a flat green sheet; damp source banks distinguishable without metallic glare; no noise shimmer at distant/grazing angles; retained continuous biome colors and source-coordinate phase; unchanged geometry/collision; scene and study classification agree. Macro bump scale, low-light readability, damp extent, interpolation on coarse far tiles and incremental shader/mesh-compilation cost require actual validation. A successful build alone is not visual acceptance.

## Cabin finding, intentionally unchanged

`Games/Sanctuary/Project/World.swift:40` initializes Cabin terrain with `Instance(kind: 3)` at line 42 and roughness .95. The garden-edit rebuild at line 212 repeats kind 3 at line 214. The existing dedicated ground recipe is kind 1 in `Games/Sanctuary/Authoring/Materials.metal`; this candidate changes neither cabin call site nor that recipe. Root will assess this separately.

## Candidate source fingerprints

`git diff --check` passed. No compile or shader check was run by this author.

- `Games/Sanctuary/Project/BiomeGroundAppearance.swift`: `63db69111bad5e968d07fcf07453cae1efb416aabd6fae1cf81c4ae0c2a7d214`
- `Games/Sanctuary/Authoring/Materials.metal`: `1fd8cca810fd7ca241bf82ee30eca635c2c104ff24499c9b95b106851448833d`
- `Games/Sanctuary/Project/BiomeWorldPresentation.swift`: `4a9779d9862867f6e6677b82e7f419ae99dd2ba491b4dcec5fef8930341dd1a6`
- `Games/Sanctuary/Project/World.swift`: `9b244434c9ebb588f0ef32fcf40f72d83879229fdbe543a767b35c27c402ce78`
- `Games/Sanctuary/Project/SanctuaryProject.swift`: `5fffae90819d50c355a603cca75b18d332a37a0522f4e114ba968b0092aa2a8c`
