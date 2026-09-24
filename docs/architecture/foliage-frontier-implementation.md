# Foliage implementation — September 23, 2026

See the [implementation report and evidence](../research/foliage-implementation-2026-09-23/README.md) and [original study](../research/foliage-frontier-2026-09-23/README.md).

Implemented: canonical paired-needle shoots; cohort and branch variation; wood collars/bark; explicit and source-derived proxy products; shared geometry and texture residency; independent camera/sun selection; conservative culling and group bounds; coherent motion and per-occurrence history; local sky-escape approximation; picking/cooking/transfer/memory integration; authoring controls; editable forest edge and player walk; bounded reference, qualification, fixed-work ABBA, native temporal capture and prepass-ablation tools.

Acceptance remains separate. The three-plane proxy fails the proposed coverage/radiance budgets, the reference spatial comparison still needs convergence, and the forest is not visually AAA. Short-run CPU performance reaches the 2 ms target; the controlled GPU comparison remains just above 6 ms. No sustained thermal/battery or new whole-crown qualification is claimed. Existing unqualified crown candidates remain gated, and compact instances are rejected by the crown builder until it understands their geometry.

Evidence and test logs are under `output/foliage-implementation-2026-09-23/`. GPU runs preserve source manifests and bundles. The first uncached instance attempt regressed CPU performance and was rejected. A shader that wrote a full sample mask even with the depth prepass also increased cost; separate color entry points preserve early depth testing. The initial disabled-prepass timing record included an unwritten query, was invalidated, and was rerun after fixing enabled-pass accounting.

The workspace has concurrent changes, particularly water rendering. They were preserved. Small WGSL signed-coordinate conversions and mock resource-pool initialization were repaired where they prevented all renderer validation. The temporary water-resource-test boundary failure was subsequently resolved; the package-boundary check now passes. Do not weaken boundaries to hide failures.

Final validation: 207 tests passed across 50 files; both TypeScript checks and package boundaries passed. The native player trail control was exercised successfully. Nine seed/age captures and 20 temporal route images were retained. Final short default run: 6.160384 ms GPU p95, 1.8 ms CPU p95, 3,072 measured frames at native 1080p on the M4 Air. The proxy remains unqualified.
