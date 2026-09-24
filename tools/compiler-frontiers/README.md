# Compiler frontiers laboratory

Research prototypes for six compiler opportunities. The follow-up promotes joint finite-periodic integration into the production `weave` material; branch traversal remains experimental. Existing materials retain their rendering paths.

| Question | Experiment |
| --- | --- |
| Can microgeometry become a lighting response? | `appearance.py`: normal/material aggregation versus 8,192 explicit samples, including sharp highlights. |
| Can a branch become a compact lighting program? | `appearance.py`: held-out relighting, low-rank failure, analytic shadow-event intervals, finite angular-line integration, deformation and precision counterexamples. |
| Can the compiler eliminate irrelevant lighting? | `probes.ts`: position balls and normal cones over real winter terrain. `gpu-fixture.ts`: generic loops, compact lists, and generated programs using the production GGX function. |
| Can it integrate motion instead of sampling it? | `fourier.ts`: exact symbolic expansion of a finite cosine expression subset; existing phase evaluator and generated WGSL. Dense independent quadrature, correlation and acceleration counterexamples. |
| Can it compile responses to edits? | `probes.ts`: actual terrain mesh generation, local support plus the normal stencil, full-rebuild equality and distant shadow changes. Production terrain already has spatial cache keys. |
| Can shared causes generate consistent content? | `appearance.py`: conservative snow/meltwater/drift/routing/refreeze toy and six derived channels, with temperature controls. |

Run from the repository root:

```sh
bun tools/compiler-frontiers/run.ts
python3 tools/compiler-frontiers/appearance.py
bun tools/compiler-frontiers/gpu-check.ts
bun test ./tools/compiler-frontiers/probes.test.ts
python3 tools/compiler-frontiers/appearance_test.py
```

Python requires NumPy and Pillow. The experiment used the desktop bundled Python at `/Users/ryanwible/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/bin/python3`; no new dependencies were installed. GPU checks use the existing hardware Chrome harness and require timestamp queries.

Results go to `output/compiler-frontiers/`. GPU runs also preserve their compiled browser bundle and source/run manifests in the returned `output/browser-*` directory. `appearance.json` records the Python implementation hash; `branch-data.npz` and `weather-data.npz` preserve reference data. The durable report and selected images are under `docs/research/compiler-frontiers-2026-09-21*`.

Measurements are deliberately scoped. These compute kernels are not scene frame times. Appearance error is relative L2 against the stated reference, not a perceptual guarantee. The branch is 240 flat plates with centroid visibility, no multiple scattering, and one fixed light-elevation orbit. Its animation is an explanatory research fixture, not a finished tree asset. Event boundaries need numerical handling; pose changes invalidate the current table. The weather model demonstrates dependency coherence and mass accounting, not validated snow physics or achieved production art quality.


The rendered follow-up is documented in `docs/research/compiler-frontiers-rendered-2026-09-21.md`. `periodic-material.ts` in the compiler is now the shared symbolic implementation; this directory's `fourier.ts` re-exports it. Run `build-material.ts --check` to detect a stale generated weave shader. `material-check.ts` checks independent perspective references, moving-camera images and three rotated native-1080p timing trials. The spatial branch path uses `branch-spatial.py --prepare`, `branch-compile.ts`, `branch-spatial.py`, then `branch-check.ts`; it explicitly tests direction-domain fallback under affine sway. Candidate pruning did not improve measured GPU throughput and is not enabled in the production renderer.
