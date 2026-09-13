# 0212 coverage contract: rejected appearance, diagnostic next

All30 actual0212 PNGs were inspected. Dry undercoat reads as rectangular raised
tiles/basket pattern over smooth skin; wet highlights strengthen those tiles.
The sampled hop and blink retain face/lining visibility and attached anatomy,
but do not soften the pattern. Before/reload images appear identical. Still
images cannot establish shimmer or exact floor contact. Archive with every PNG
and metadata SHA:
`.build/sanctuary-native-20260912/undercoat-0212-review/review.json`.
Default remains off and there is no accepted baseline or cost qualification.

## What the implementation actually does

`MetalRenderer.upload` detects any vertex.groom.z>0. Draw selection chooses the
groom pipeline before material-specialized pipelines. The groom fragment sets
function constant2 true; both vertex/mesh paths enable alpha-to-coverage.
`surfaceFragment` evaluates coverage, discards values≤.001 and returns coverage
in alpha. Vertex evaluation preserves the four groom floats. This source trace
does not indicate a missing pipeline flag. Actual sample-mask receipt is still
needed to prove GPU behavior, rather than interpreting a bright rectangle as
proof that alpha was ignored.

The footprint integral intentionally converges toward average density. With
0.8mm intervals and density.94, an unresolved patch interior approaches94%
coverage. At the quarter framing, a frontoparallel estimate gives3.052 strand
periods/pixel; the body patch itself spans22.5×27.4pixels. Actual per-fragment
depth, tilt and derivatives differ and must be measured natively.

The existing interface carries strand phase, progress, density and tip variation.
It has **no transverse patch-domain boundary**: its phase is periodic across the
width, and only the along-strand tip fades. At a side edge the mesh stops with
about94% interior coverage. The lifted geometry and its changed normals remain
at centimetre scale after the subpixel strands have averaged away. This explains
why a correctly filtered strand signal can still expose a rectangular tile.
Increasing density would drive it closer to an opaque tile. High retained-vertex
exposure is not the same as continuous projected coat coverage.

Kind13 remains an anisotropic surface BRDF, with roughness.86 and no project
bump. Generic wet response lowers roughness; the broad specular response from
the substrate remains visible between patches. There is no volume extinction,
multiple scattering, wet geometric clumping or translucent membrane model.
The previously retained Lengyel et al. silhouette/coverage distinction and
Khronos aggregate sheen comparison remain relevant; neither makes this patch
representation valid merely because its sampled geometry tests pass.

## Reproducible bounded scalar diagnostic

```sh
python3 Tools/Experiments/SanctuaryFur/coverage_contract.py --out .build/sanctuary-native-20260912/undercoat-coverage-contract
```

This dependency-free transcription ran in0.127s. CSV sweeps four densities,
seven footprints and five progress values; the receipt preserves script/source
hashes. The scalar shader block was checked identical between main and0212.
At footprint3.05 and progress.25, density.94 gives mean.9399998,
min.9245902,max.9409836; at progress.9 mean.2712475; at the exact tip0.
An integer8-period footprint returns the exact expected mean density in the
tested scalar arithmetic. These are double-precision CPU expectations, **not**
Metal numerical qualification, alpha-to-coverage mask measurements or rendered
fur evidence. Four MSAA samples allow five coverage-count levels; their mapping,
dithering and overlap correlation are implementation dependent and unmeasured.

## Smallest next native test and ownership request

Hold current Sunhare geometry and shader math. Author one isolated coverage
subject containing a flat and a lifted55×67mm patch from the same sampled source
formula, with known0.8mm periods and explicit density.25/.5/.75/.94. Put them
over a contrasting opaque substrate. Use the same material/pipeline and three
diagnostic render modes: ordinary kind13, unlit analytic coverage, and opaque
unlit boundary reference. This is a calibration subject, not another coat.

Requested ownership: new `SanctuaryCoatCoverageStudy.swift`; narrow diagnostic
branch in `Surface.metal` selected solely by a reserved study material kind;
registration through root. No edit is made before that handshake. Diagnostic
mode must retain the ordinary coverage calculation and A2C pipeline while
returning a known unlit color before lighting/tone contribution. A disabled-
coverage reference must still use the same mesh. No night/sky/other material
branch, global exposure, ABI or production recipe changes are needed.

Root captures1080p4×MSAA at distances.25m (mostly resolved),2.2762m (unresolved)
and4m, front and grazing. Exact source/UV geometry makes expected footprint
calculable in a flat front view. Native sample-mask or linear resolve readings
are preferable; PNG values require the known display transform and black/white
calibration and must not be treated directly as linear coverage.

Acceptance: density ordering is monotone; resolved intervals/tip shortening
appear; unresolved mean matches expected coverage to the documented4-sample
quantization/resolve tolerance; zero tip reveals substrate; opaque reference
shows exact polygon boundaries. Compare flat versus lifted patches to isolate
geometric-normal discontinuity from alpha. If coverage fails, fix the demonstrated
pipeline/filter issue before any coat change. If it works, reject the current
centimetre-scale patch boundary contract and design a boundary-aware coverage
representation before revisiting fur identity. No density increase or guessed
patch-shape revision is supported by this result.
