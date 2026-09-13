# Executable coverage calibration handoff

Source-ready, not compiled or natively reviewed. Root runs the matching copied
source; no production coat geometry changes accompany this diagnostic.

```sh
swift test --filter 'CoatCoverageStudyTests'
```

This selects three tests using the actual registered generator. They check that
all modes preserve mesh positions/normals/colors/coverage inputs, finite outward
geometry, production coverage means/tips, deterministic input handling and a
40,000-byte mesh cap. Native shader/readback checks remain required separately.
Predicted source totals:466 vertices,776 triangles,39,136 raw bytes in3 batches.
Existing geometry arrays are static; compilation copies only small control data.

```sh
export WRELA_CONTROL_ROOT=/absolute/matching/workspace/.soundstage
./scripts/stagectl replay Tools/Experiments/SanctuaryFur/coverage-calibration-replay.json > "$WRELA_CONTROL_ROOT/coverage-calibration-receipt.json"
```

The23 captures use the `coat-coverage` catalog subject, softbox, scale1, ground
off and ordinary shared display/exposure. Two55×67mm white patches sit over a
black board: **left planar, right lifted** using the0212 along-guide elevation
formula. Both taper width by18% toward the tip. The lower chips encode linear
0,.18,1 from left to right. The substrate is6mm behind the flat surface.

`coverageMode=0` uses ordinary kind13 lighting. Mode1 uses reserved kind30 and
returns unlit vertex RGB with analytic coverage in alpha. Mode2 uses kind31,
returning the same unlit RGB with alpha1 before discard. Crucially, both patch
meshes retain nonzero groom.z in every mode, so the existing A2C pipeline and
all vertex/UV data remain the same. The black/reference substrate has groom0
and kind31. No global exposure, ABI, sky or production material is changed.

The first18 captures compare all three modes at distances.25,2.2761991,4m,
front and70° grazing (orbit1.221730476,elevation0). Camera settings are reapplied
after source/view changes. The next3 compare density.25,.5,.75 in front analytic
mode at2.2761991m, supplementing the existing.94 frame. The last2 repeat ordinary
and analytic modes wet at that native framing. All use0.8mm fibre interval.

Read the native camera metadata before comparing. At front target-depth, the
expected strand footprints are approximately.335,3.052,5.363 periods/pixel.
The actual flat patch depth is offset from the bounds center by a few mm; use
the recorded eye/target or opaque rectangle's projected width for exact pixel
footprint. Grazing patches have varying perspective depth and cannot use the
frontoparallel estimate. Mode2 reveals actual polygon bounds without coverage.

The scalar expectation is reproducible with `coverage_contract.py`. At an
integer8-period footprint and interior progress.25, mean coverage equals source
density; tip coverage is0. At3.05 periods and density.94, the predicted mean is
.9399998 and phase range.9245902–.9409836. Resolved close views should show
individual stripe openings; unresolved views should approach those means.
This diagnoses sampling only, not convincing fur.

Read **linear resolved color or sample masks** if available. Do not directly
compare PNG/sRGB values with alpha: tone mapping/display encoding is nonlinear.
The gray/white chips provide display calibration but are not enough to assume
an arbitrary transfer function. With4×MSAA only five sample-count levels exist;
the implementation's alpha mapping, spatial dither and overlapping masks are
not assumed. At minimum require monotone density response, visible substrate at
zero tips and opacity changes between analytic and opaque modes. A quantized
nearly opaque.94 result alone is not a failed shader.

Compare flat/lifted analytic masks to isolate boundary geometry; compare ordinary
mode to analytic mode to isolate BRDF/normal contrast. If analytic behavior is
correct, the old rectangular coat remains rejected because its large boundaries
and lifted normals dominate. Do not alter density or promote fur based on a
successful calibration. Source and diagnostic-block hashes are retained in
`coverage-calibration-source.json`; no tests/GPU were run by this author.
