# Conformal undercoat: source boundary and next gate

## Current0300 source checkpoint

Root copied the transverse coverage-envelope correction into immutable0300.
Only coat groom.z changed; source positions/indices/normals, placement, materials
and defaultoff remain exact. Existing four tests add production edge/center
coverage probes. Compilation and new30-image review are pending. Read
`COVERAGE_0237_DECISION.md` for the measured reason and limitations.
Previous0212 compiled metrics below refer to that historical boundary.


## Actual0212 native rejection

All30 matching PNGs inspected: rectangular raised tiles/basket pattern remains
over smooth skin, with stronger wet plastic highlights. Face/lining and sampled
parent motion remain readable, but fur identity fails. Default stays off; no cost
profile or baseline promotion. Hash archive:
`.build/sanctuary-native-20260912/undercoat-0212-review/review.json`.
Next diagnostic and exact scalar replay are in
`Tools/Experiments/SanctuaryFur/COVERAGE_DIAGNOSIS.md`; no geometry revision follows
from the CPU pass.


## Actual0212 compiled qualification

The current adaptive source passed all four production coat tests in32.455s;
the focused run passed21 tests with zero failures. Actual default retained-vertex
exposure is0.79492533, maximum sampled reduction error0.0009913478m, raw mesh
bytes484,448. All three owned source/test hashes exactly match immutable0212
and the checkpoint. Receipt, copied source and log SHA are retained in
`.build/sanctuary-native-20260912/undercoat-0212-qualification/compiled-receipt.json`.
This supersedes earlier uncompiled/estimated-count status below. Exact adaptive
vertex/triangle counts were not printed; do not infer them from raw bytes. Native
images, material identity and live cost remain unqualified; default remains0.


Current source is **uncompiled and unrendered**, opt-in `furStudy=1`; default0
retains every base vertex and the shared face. `coatStudy=0` isolates this geometry
comparison from the separate localized material experiment. Source hashes and
caps are in `groom-source-checkpoint.json`. No shader/ABI change accompanies it.

Root should run from the matching isolated candidate, compiling changed source:

```sh
swift test --filter 'SunhareCoatGeometryTests'
```

This class contains exactly four production mesh/rig tests:

1. `testRegisteredOptInPreservesEveryBaseVertexAndSeparateMaterialStudy`
2. `testActualCompiledRootsMeetParentsAndRespectFaceLiningAndSoleExclusions`
3. `testBoundedDeterministicCompilerAcrossSemanticCornersAndCacheEviction`
4. `testActualGroomVerticesInheritAnimatedParentsWithoutChangingBaseRig`

Retain actual counts, raw bytes, exposure fraction and maximum reduction error
from that run. Source predicts352 conformal patches in4 parent batches,
adaptive vertices/triangles within16,896 triangles and1MiB. Bounds remain1MiB per recipe,
4MiB/four-recipe cache. These are not measured GPU memory or compile-time costs.
Projection samples the actual unchanged parent fields; approximation compares
the actual coarse triangles against a25×9 source grid, rejecting deviation over
1mm. This finite sample gate is not a global mathematical error certificate.
Tests also cover real compiled parent roots, face/lining/sole exclusions,
semantic corners, deterministic cache eviction and animated parent inheritance.

After that gate, root runs the existing matched30-capture recipe in an isolated
Soundstage workspace associated with the same candidate:

```sh
export WRELA_CONTROL_ROOT=/absolute/matching/workspace/.soundstage
./scripts/stagectl replay Tools/Experiments/SanctuaryFur/groom-acceptance.json > "$WRELA_CONTROL_ROOT/undercoat-acceptance-receipt.json"
```

The recipe retains the registered Sunhare, original brain/pose, seed17, softbox,
front/quarter/left dry/wet pairs, six side hop samples, idle/blink and exact
study reload. Verify each pair's physical camera, resolved light, time,
source/shader digest,1080p dimensions and4×MSAA. Historical0130 auto-follow gave
front2.19234m,quarter2.27620m,left1.82885m; pair equality is required, not an
unsupported claim that every view is2.276m. No baseline or published asset save.

## Existing renderer assumptions

`Vertex.groom` is the existing four-float interface. Nonzero density selects the
existing groom pipeline; `Surface.metal` evaluates `groomCoverage`, discards
near-zero coverage and writes alpha for MSAA alpha-to-coverage. No sorted
transparency, shell stack or new shading ABI is introduced. Current patch data
uses0.8mm cross-groom periodic spacing,0.94 center duty tapering to0.0001
at the two sides, and0.18 length variation;
the existing analytic pulse integral and unresolved-end blend filter these at
native derivative footprints. CPU tests exercise the same `GroomCoverage`
arithmetic, but cannot certify the GPU derivatives or sample-mask appearance.

Existing kind13 supplies anisotropic surface response from the groom gradient,
roughness0.86 and no bump. The existing wet response darkens color and lowers
roughness; this candidate adds no wet flattening/clumping. Broad wet plastic
highlights remain a specific rejection risk. Patches do not cast extra shadows.
Coverage is an unresolved fibre surface approximation, not a volume extinction
or multiple-scattering model. Overlap/depth/alpha-to-coverage artifacts and
overdraw cost remain native gates. Compilation and projection occur only in the
bounded recipe cache; no per-frame source sampling is intended.

## Decisive acceptance and provenance

Reject if the candidate remains isolated bristles on smooth clay, becomes a
striped cloth/woven surface, exposes floating patch edges, obscures the face,
enters the lining, changes contact, or sparkles in live motion. Soft continuous
coverage must read at the matched interaction framing before profiling. Actual
0130 tube geometry failed dry and wet fur identity; all30 inspected images and
hashes are in `.build/sanctuary-native-20260912/groom-0130-review/review.json`.
Its four-test pass is historical evidence only, not a pass for this candidate.

If appearance passes, use the short matched live procedure in `GROOM_REPLAY.md`.
Incremental provisional limits remain GPU p95+0.20ms,CPU update p95+0.05ms with
normal cache work. Latest populated eye-level reference already misses60Hz:
GPU median/p95/max25.576/32.937/37.668ms, thermal fair. See
`.build/sanctuary-native-20260912/groom-parity-0017-populated-profile-analysis.md`.
Studio cost alone cannot qualify that scene or promote the default.

0208 compiled but rejected body patch0 at1.4372791mm. Current correction retains
source rows/columns at the worst-error sample until the same1mm triangular
approximation gate passes. It starts5×3 and may refine to the finite25×9 source
grid, without changing352 source patches or weakening global mesh/cache caps.
Actual total budget and exposure still require the next compiler run; no passed
receipt is implied. Per-guide ranges replace fixed35-vertex indexing in tests.

## Actual0306 gate

All30 images inspected; all14pairs match, study reload pixels identical.
Four0300coat tests passed32.167s; exact owned source retained0306.
Appearance remains rejected: softer raised spots/dashes over smooth skin, wet
plastic highlights; face and sampled poses retained. No default promotion.
Archive `.build/sanctuary-native-20260912/groom-0306-review/review.json`.
No further geometry change follows this review.
