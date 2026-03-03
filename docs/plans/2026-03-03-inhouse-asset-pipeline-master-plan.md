# Wrela Master Plan: In-House AI-First Asset Pipeline (Stylized AAA Vertical Slice)

## 1) Mission

Build a fully in-house, AI-first asset pipeline inside Wrela that takes plain-English intent and produces deterministic, validated, runtime-ready game assets for a stylized high-quality vertical slice.

This plan is a hard cutover plan:

1. No backwards compatibility lanes.
2. No mixed legacy/canonical artifact contracts.
3. No permissive fallbacks at runtime.

## 2) Product Outcome

Ship one polished vertical slice where all slice assets are created through the new pipeline:

1. Hero character.
2. Enemy set (at least one standard enemy and one boss).
3. Environment modules/props.
4. Terrain patch.
5. Foliage cluster set.

All assets must pass deterministic conditioning, style consistency checks, performance budgets, and provenance policy before runtime load.

## 3) Scope Constraints

1. External SaaS generation is out of scope for this program.
2. The user workflow is prompt-first: describe intent, choose variants, iterate.
3. Art direction target is stylized (not photoreal).
4. Runtime ingest target remains GLB for this program.
5. We optimize for vertical-slice quality first, not full-game volume.

## 4) Current Baseline (Repo Truth)

Wrela already has strong foundational primitives:

1. Deterministic asset request/envelope/cache surfaces:
   1. `compiler/asset_factory/mod.rs`
   2. `compiler/resolve/mod.rs`
2. GLB ingest and skeletal load path:
   1. `client/src/mesh.rs`
   2. `client/src/skeletal_animation.rs`
3. Mesh/texture conditioning primitives:
   1. `mesh_tooling/src/lod.rs`
   2. `texture_tooling/src/artifact.rs`
4. Streaming/pack manifest contracts:
   1. `asset_pack/src/pack.rs`
5. Fail-closed runtime manifest/provenance validation:
   1. `client/src/manifest_validation.rs`
6. Build-time artifact/provenance report generation:
   1. `compiler/bin/wrela/commands/game.rs`

## 5) End-State Architecture (Decision Complete)

1. Control Plane:
   1. Prompt -> `AssetSpec` -> deterministic job envelope -> adapter execution.
2. Generation Plane:
   1. Local model adapters for mesh/texture variants.
3. Conditioning Plane:
   1. Mesh validation + LOD lineage + deterministic hashes.
   2. Texture packing + mip chain + streaming metadata.
4. Packaging Plane:
   1. Canonical asset pack and world chunk manifests.
5. Policy Plane:
   1. Provenance, license class, attestation, deterministic evidence.
6. Runtime Plane:
   1. Only validated canonical artifacts load; no bypass path.

## 6) Program Phases

## Phase A: Canonical Contract Freeze (Hard Cut)

Freeze and enforce canonical contracts for:

1. Asset generation requests/envelopes/results.
2. Mesh LOD/lineage metadata.
3. Texture artifact/mip/streaming metadata.
4. Asset pack/world chunk manifests.
5. Asset factory/provenance/quality manifests.

Deliverable:

1. Single canonical schema path across compiler/tooling/client.

Gate:

1. Any legacy schema/kind/version path removed or rejected.

Canonical schema table (must be enforced exactly):

| Artifact | Kind | Schema Version | Source Module | Reject Behavior |
|---|---|---|---|---|
| Asset Factory Manifest | `asset-factory-manifest-v2` | `2` | `compiler/bin/wrela/commands/game.rs`, `client/src/manifest_validation.rs` | Hard fail on mismatch |
| Asset Provenance Ledger | `asset-provenance-ledger-v1` | `1` | `compiler/bin/wrela/commands/game.rs`, `client/src/manifest_validation.rs` | Hard fail on mismatch |
| Asset Quality Report | `asset-quality-report-v2` | `2` | `compiler/bin/wrela/commands/game.rs`, `client/src/manifest_validation.rs` | Hard fail on mismatch |
| Asset Pack Manifest | `asset_pack_manifest_v4` | `4` | `asset_pack/src/pack.rs`, `client/src/manifest_validation.rs` | Hard fail on mismatch |
| World Chunk Manifest | `world_chunk_manifest_v3` | `3` | `asset_pack/src/pack.rs`, `client/src/manifest_validation.rs` | Hard fail on mismatch |
| LOD Chain | `lod-chain-v2` | `2` | `mesh_tooling/src/lod.rs` | Hard fail in conditioning |
| Texture Artifact | `texture_artifact_v1` | `1` | `texture_tooling/src/artifact.rs` | Hard fail in conditioning |

## Phase B: Local Adapter Orchestration

Build deterministic local generation adapter surfaces and wire resolve DAG:

1. Generate.
2. Condition.
3. Validate.
4. Emit canonical references.

Deliverable:

1. Prompt-to-asset local flow with reproducible envelopes/hashes.
2. Every generated artifact includes reproducibility metadata:
   1. `model_id`
   2. `model_version`
   3. `weights_digest`
   4. `runtime_backend`
   5. `sampler`
   6. `steps`
   7. `guidance`
   8. `seed`
   9. `adapter_version`

Gate:

1. Unsupported model/capability combinations fail before execution.
2. Identical model metadata + seed must produce metric-identical outputs or fail determinism gate.

## Phase C: Production Conditioning and Class-Aware Gates

Upgrade mesh/texture and class-specific quality gates:

1. Characters.
2. Props.
3. Terrain.
4. Foliage.

Deliverable:

1. Fail-closed class-aware quality system with machine-readable diagnostics.

Gate:

1. No asset can pass with missing conditioning evidence or class-specific threshold failures.

## Phase D: Runtime Enforcement and Packaging Lockdown

Harden packager + runtime validation:

1. No runtime path without valid pack/manifests/provenance.
2. No direct mesh load bypass around manifest policy.

Deliverable:

1. Runtime acceptance only for canonical conditioned artifacts.

Gate:

1. Runtime boot fails closed on any policy or schema violation.

## Phase E: Vertical Slice Asset Production and Final Gate

Generate and ship slice content through the new pipeline only.

Deliverable:

1. Release-quality vertical slice artifact set + passing final gate report.

Gate:

1. Deterministic rebuild parity + performance + style + provenance all green.

## 7) Parallel Worklanes (Tasks, AC, Tests)

## WL1 - Asset Factory Core Hard Cut

Owned modules:

1. `compiler/asset_factory/mod.rs`
2. `compiler/asset_factory/tripo.rs` (replace with local adapter lane)

Tasks:

1. Introduce canonical local adapter registry with explicit capability contracts.
2. Include model/version fingerprint in replay hash inputs.
3. Require structured artifact metadata fields:
   1. `prompt_digest`
   2. `model_id`
   3. `model_version`
   4. `weights_digest`
   5. `runtime_backend`
   6. `sampler`
   7. `steps`
   8. `guidance`
   9. `seed`
   10. `adapter_version`
   11. `conditioning_contract_version`
4. Remove ambiguous provider defaults and permissive adapter behavior.

Acceptance Criteria:

1. Same request + same model version yields byte-identical envelope and metadata.
2. Unsupported adapter/model combos fail preflight.
3. Artifact results never omit required metadata keys.
4. Two runs with identical model metadata and seed produce metric-identical outputs (or fail determinism gate).

Tests:

1. Determinism snapshots for request -> envelope -> result.
2. Negative tests for missing capability/model.
3. Contract serialization tests with strict unknown-field rejection.

## WL2 - Resolve DAG and Failure Semantics

Owned modules:

1. `compiler/resolve/mod.rs`

Tasks:

1. Implement deterministic DAG sequencing (generate -> condition -> validate -> publish references).
2. Add structured per-asset failure records.
3. Remove permissive unresolved behavior.
4. Enforce deterministic output ordering independent of parallelism.

Acceptance Criteria:

1. Failed assets never emit runtime paths.
2. `parallel=1` and `parallel=n` produce same ordered output and hashes.
3. Dry-run planning hashes match execute mode hashes.

Tests:

1. Concurrency determinism tests.
2. Mixed success/failure integration tests.
3. Dry-run/execute parity tests.

## WL3 - Style Authority and Prompt-to-Spec Layer

Owned modules:

1. `compiler/hir/def.rs`
2. `compiler/hir/lower.rs`
3. `compiler/parser/*` (asset/style declarations)

Tasks:

1. Formalize style authority contract:
   1. Palette rules.
   2. Material family constraints.
   3. Shape/silhouette tags.
   4. Forbidden style tags.
2. Extend `AssetSpec`/declaration surfaces with required style policy refs.
3. Enforce that every generation request binds a style profile id.

Acceptance Criteria:

1. Any asset request without style profile is rejected.
2. Style policy contracts compile into deterministic generation inputs.
3. Style violations are reported as explicit gate diagnostics.

Tests:

1. Parser/lower tests for style policy declarations.
2. Semantic tests for missing/invalid style profile references.
3. Resolve integration tests confirming style profile propagation.

## WL4 - Mesh Conditioning Production Gates

Owned modules:

1. `mesh_tooling/src/lod.rs`
2. `mesh_tooling/src/types.rs`

Tasks:

1. Add stricter mesh topology validation classes:
   1. Degenerate triangles.
   2. Index/attribute mismatch.
   3. Empty surface and bounds validity.
2. Extend LOD chain constraints:
   1. Monotonic triangle reduction.
   2. Deterministic lineage hash requirements.
   3. Class-dependent minimum LOD count policy.
3. Add skeletal guardrails for joint count and max influences metadata checks.

Acceptance Criteria:

1. Invalid topology cannot produce a valid LOD chain.
2. LOD chain rules and lineage hashes are deterministic.
3. Character-class assets with invalid skinning metadata fail closed.

Tests:

1. Unit tests for each invalid topology class.
2. Determinism tests across repeated LOD generation runs.
3. Fixture tests for malformed and valid skeletal metadata envelopes.

## WL5 - Texture Conditioning Production Gates

Owned modules:

1. `texture_tooling/src/artifact.rs`
2. `texture_tooling/src/mips.rs`
3. `texture_tooling/src/types.rs`

Tasks:

1. Enforce strict channel pack schema and source binding checks.
2. Require full mip chain integrity and deterministic source/content hashes.
3. Enforce streaming metadata policy by asset class.
4. Emit measured preservation statistics for configured channels.

Acceptance Criteria:

1. Invalid channel pack or stream metadata fails artifact build.
2. Same input program/layout produces deterministic content hash.
3. Required mip/preservation evidence is always present.

Tests:

1. Golden tests for channel packing layouts.
2. Mip determinism and preservation tests.
3. Negative tests for invalid stream metadata and source binding errors.

## WL6 - Terrain and Foliage Specialized Pipeline

Owned modules:

1. `compiler/asset_factory/*`
2. `asset_pack/*`
3. `client/src/manifest_validation.rs`

Tasks:

1. Introduce terrain-specific generation/conditioning contract:
   1. Traversable slope bounds.
   2. Chunk/HLOD requirements.
   3. World partition compatibility metadata.
2. Introduce foliage-specific contract:
   1. Density bands.
   2. Impostor/billboard requirements.
   3. Overdraw/perf metadata.
3. Add class-specific gate diagnostics and blockers.

Acceptance Criteria:

1. Terrain assets missing traversal/HLOD metadata fail.
2. Foliage assets missing impostor/perf metadata fail.
3. Class-specific manifests are validated end-to-end.

Tests:

1. Terrain/foliage manifest validation matrix tests.
2. Partition/refinement consistency tests in `wrela_asset_pack`.
3. Runtime validation tests for terrain/foliage policy presence.

## WL7 - Character, Rig, and Animation Quality Gates

Owned modules:

1. `client/src/skeletal_animation.rs`
2. `client/src/manifest_validation.rs`
3. `compiler/bin/wrela/commands/game.rs`

Tasks:

1. Enforce character rig contract metadata:
   1. Skeleton schema id.
   2. Joint count and influence caps.
   3. Retarget compatibility tags.
2. Add animation quality metrics in quality report:
   1. Foot sliding.
   2. Root drift.
   3. Transition pop indicators.
3. Add hard fail for missing character quality evidence on character classes.
4. Replace synthetic quality evidence fields with measured metrics emitted from real conditioning outputs.

Acceptance Criteria:

1. Character assets without rig/animation evidence are blocked.
2. Animation quality report includes measured, not synthetic, metrics.
3. Runtime load path receives validated character contract fields.

Tests:

1. Character manifest failure tests for missing rig evidence.
2. Skeleton/clip compatibility tests in loader fixtures.
3. Quality report schema and evidence-presence tests.

## WL8 - Packaging and Runtime Lockdown

Owned modules:

1. `asset_pack/src/pack.rs`
2. `client/src/manifest_validation.rs`
3. `client/src/mesh.rs`

Tasks:

1. Tighten pack/world validation:
   1. Deterministic ordering.
   2. Budget arithmetic integrity.
   3. Partition coverage and dependency integrity.
2. Enforce runtime canonical load policy only through validated manifests.
3. Remove any direct non-canonical runtime ingest path.

Acceptance Criteria:

1. Pack/world mismatch or budget violation always fails.
2. Runtime cannot load non-manifested/non-canonical assets.
3. All accepted assets show complete conditioning + provenance metadata.

Tests:

1. Cross-manifest validation integration tests.
2. Runtime rejection tests for bypass and malformed artifacts.
3. Canonical fixture acceptance tests.

## WL9 - Determinism, CI, and Final Gate

Owned modules:

1. `scripts/*` gate harnesses
2. `compiler/bin/wrela/commands/game.rs`

Tasks:

1. Build one end-to-end gate command for asset pipeline verification.
2. Add deterministic rebuild parity checks.
3. Add class-aware quality threshold checks.
4. Emit one release eligibility report.
5. Reject synthetic placeholder evidence in factory/quality manifests.

Acceptance Criteria:

1. Two clean runs produce identical manifest hashes and deterministic metadata.
2. Gate fails on any class threshold violation.
3. Release report contains explicit pass/fail reasons and blocker ids.

Tests:

1. CI e2e test for gate command.
2. Rebuild parity test.
3. Performance smoke tests for slice scenes.

## 8) Risk Register

1. R1 Style drift across asset classes.
2. R2 Hero topology/deformation failures.
3. R3 Skeleton/weights incompatibility.
4. R4 Non-deterministic model behavior breaks reproducibility.
5. R5 LOD pop/silhouette degradation.
6. R6 Terrain traversal/readability failures.
7. R7 Foliage overdraw/perf collapse.
8. R8 Texture memory/streaming budget blowups.
9. R9 Provenance/legal contamination.
10. R10 Synthetic quality reports mask true quality regressions.
11. R11 World coherence failures from prompt-only generation.
12. R12 Local hardware throughput bottlenecks.

Each risk maps to a hard gate in this plan. No risk is accepted silently.

## 9) Quality Gate Matrix (Threshold Summary)

Global blockers:

1. Any provenance error (`unknown_lineage`, `blocked_license`, `missing_attestation`).
2. Any schema/kind/version mismatch.
3. Missing deterministic hash/conditioning evidence.
4. Missing class-specific required evidence.

Class thresholds:

1. Character:
   1. LOD >= 4.
   2. LOD0 triangle budget <= 120000 (hero), <= 45000 (enemy).
   3. Material slots at LOD0 <= 3.
   4. Texture set cap <= 4K (hero), <= 2K (enemy).
   5. Joint count <= 128 (hero), <= 64 (enemy).
   6. Max influences <= 4.
   7. Foot slide <= 2 cm per frame average over validation clip set.
   8. Root drift <= 3 cm per 10 seconds on in-place validation clips.
   9. Transition pop count == 0 on required transition matrix.
2. Props:
   1. LOD >= 3.
   2. LOD0 triangle budget <= 25000.
   3. Material slots at LOD0 <= 2.
   4. Texture set cap <= 2K.
   5. Silhouette error at LOD switch <= 7%.
3. Terrain:
   1. Walkable slope <= 35 degrees in playable traversal zones.
   2. Step height discontinuity <= 0.40 m in playable traversal zones.
   3. Chunk/HLOD levels >= 3.
   4. Terrain texture tile cap <= 4K per tile.
4. Foliage:
   1. LOD levels >= 4 including impostor/billboard tier.
   2. Impostor mandatory beyond 30 m camera distance.
   3. Material slots <= 2.
   4. Texture atlas cap <= 1K for non-hero foliage clusters.
   5. Overdraw cost from foliage <= 20% of frame GPU time in slice perf scene.

Global runtime/perf thresholds:

1. Target framerate >= 60 FPS in slice perf scenes.
2. 99th percentile frame time <= 20 ms.
3. Asset streaming stall budget <= 50 ms per 5 minutes of gameplay.
4. Cold asset load budget <= 200 ms per asset in representative slice hardware profile.

## 10) Dependency Graph and Execution Order

Start in parallel:

1. WL1.
2. WL3.
3. WL4.
4. WL5.
5. WL7.
6. WL8.

Then:

1. WL2 after WL1/WL3/WL4/WL5 contracts stabilize.
2. WL6 after WL4/WL5/WL8 policy surfaces stabilize.

Then:

1. WL9 after WL2/WL6/WL7/WL8 complete.

No lane may add compatibility shims to bypass blocked dependencies.

## 11) Vertical Slice Exit Criteria

The vertical slice is accepted only when all are true:

1. Content checklist complete:
   1. 1 hero character.
   2. 1 standard enemy class.
   3. 1 boss class.
   4. 1 terrain patch with partition/HLOD metadata.
   5. 1 foliage cluster set with impostor tier.
2. Every asset in slice is produced through canonical pipeline and passes class-specific gates.
3. Zero provenance violations (`unknown_lineage`, `blocked_license`, `missing_attestation`).
4. Deterministic rebuild parity passes on two clean runs.
5. Runtime perf thresholds from Section 9 pass in representative slice scene.
6. Visual gate suite passes 100% of required camera sweep tests.

## 12) Test Command Matrix

Core module tests:

1. `cargo test -p wrela --lib asset_factory`
2. `cargo test -p wrela --lib resolve`
3. `cargo test -p wrela_mesh`
4. `cargo test -p wrela_texture`
5. `cargo test -p wrela_asset_pack`
6. `cargo test -p wrela_client manifest_validation`
7. `cargo test -p wrela_client mesh`
8. `cargo test -p wrela_client skeletal_animation`

Pipeline and gate commands:

1. `wrela game build <app-path> --client-runtime=compiled --shader-provenance --no-shortcuts`
2. `wrela game check <app-path> --client-runtime=compiled --shader-provenance --no-shortcuts`

Expected required artifacts after successful build:

1. `dist/asset-factory-manifest-v2.json`
2. `dist/asset-provenance-ledger-v1.json`
3. `dist/asset-quality-report-v2.json`
4. `dist/assets-manifest.json` (must contain `kind = asset_pack_manifest_v4`, `schema_version = 4`)
5. `dist/world-chunks.json` (must contain `kind = world_chunk_manifest_v3`, `schema_version = 3`)

## 13) Definition of Done

This program is complete only when:

1. All worklane acceptance criteria are met.
2. All worklane tests pass in CI.
3. End-to-end vertical slice gate passes.
4. Deterministic rebuild parity passes.
5. Independent review confirms:
   1. correctness.
   2. architecture maintainability.
   3. performance viability.
   4. full task completion against this plan.

## 14) Immediate Next Step (3D Generation Program Entry)

Start with WL1 + WL3 + WL4 in parallel:

1. WL1 gives canonical local generation contracts.
2. WL3 locks style authority and prompt-to-spec mapping.
3. WL4 ensures generated geometry cannot bypass quality gates.

This gives the fastest path to begin real 3D generation work without accumulating unrecoverable tech debt.
