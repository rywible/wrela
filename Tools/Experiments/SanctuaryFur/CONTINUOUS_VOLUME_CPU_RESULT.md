# Continuous coat CPU result: HOLD

One seed37011 run of `continuous_volume_cpu.py` took **1.198 seconds** with
NumPy2.3.5. This is source-space arithmetic and a diagnostic mesh, not Swift
compiler output, native renderer evidence or an appearance approval. No
production models, shader or defaults changed.

Replay from the repository root:

```sh
/Users/ryanwible/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/bin/python3 \
  Tools/Experiments/SanctuaryFur/continuous_volume_cpu.py \
  --out .build/sanctuary-native-20260912/fur-continuous-volume-cpu
```

The output retains `metrics.json` with script/source SHA256, all angle/layer
counters, and `body-chart.npz` with1,026 vertices and2,048 triangles. A subsequent
read-only analysis of those arrays recorded `worst-geometry-sample.json`; it did
not rerun coverage or author a different representation.

## Geometry finding

The prototype transcribes the default Sunhare body's three stretched-sphere
fields and smooth unions, including FieldCore's minimum-radius distance scale.
All1,024 sampled radial rays had one outward crossing, and every chart origin
was inside. The finite radial samples do not prove global star-shapedness.

The proposed uniform32×32 chart **fails** the sampled1mm surface-accuracy gate:
maximum18.458mm, p952.351mm. The worst sample is an edge midpoint near the
chest/neck transition, not a fur-tip or alpha problem. Its triangle spans
Y.2536… .4568 while Z spans−.2387…−.2035. Selecting the deepest field point as
each ring's independent center creates an abrupt chart-center change; neighboring
angle labels no longer correspond to a smooth surface direction. The midpoint
is17.698mm inside the source field, and projection travels18.458mm to its zero
set. This is a failure of this specific centerline/proxy, not proof that the
body cannot fit any2,048-triangle mesh. The maximum vertex field residual is
.216mm; vertex-only checks would have missed the larger between-vertex error.

Do not extrude shells from this proxy. A future correction would first require
a continuous source-derived centerline or reuse of a qualified compiled surface,
with the same triangle-interior error test. This run does not authorize that
correction or a general parameterization framework.

## Coverage finding

The source defines128 Gaussian cross-section guides in a16mm periodic tile,
500,000 roots/m², sigma radius.15mm,8mm depth and a fixed comb with seeded bends.
This density and extinction are illustrative authored test parameters, not
measured animal-fur physics. Ninety-six rays compare four/eight/twelve midpoint
samples to1,024 samples through the same guide field. No screen-footprint filter,
texture bake, fin integration or native A2C is executed.

| View angle from normal | Eight-layer mean absolute coverage error | Twelve-layer error |
| --- | ---: | ---: |
| 0° | 0.66 percentage points | 0.19 points |
| 60° | 20.24 points | 13.01 points |
| 80° | 31.25 points | 16.09 points |

Thus a near-normal aggregate pass does not qualify the representation. Even
eight layers at0° have an11.49-point worst ray error. Unfiltered grazing samples
miss narrow fibres between layers. A useful next representation must integrate
the represented slab/footprint and supply grazing support from the same source;
increasing local patch density or adding a sheen cannot correct this sampling
error. Fin integration remains untested, not assumed successful.

A separate arithmetic counterexample quantifies correlated coverage masks.
For uniform optical depth1.6, ideal coverage is79.81%. Giving all layers the
same nested four-sample thresholds yields25% at four/eight layers and0% at
twelve layers, despite correct per-layer Beer attenuation. This is **not a
measurement of Metal's sample-mask pattern**. It establishes why existing
single-layer A2C calibration cannot prove correct stacked-volume composition.
Root would need an explicit coverage/compositing interface and native slab test
before any shell study. No silent reinterpretation of existing groom fields.

## Decision against the rejected patch family

Actual0306 patches and material combinations remain rejected for spots over
clay. This CPU study addresses a different uncertainty—connected domain and
volume sampling—but currently fails its own geometry and angular coverage gates.
It therefore supplies no new coat to capture or promote. Keep the liked base
Sunhare with `furStudy=0, coatStudy=0`. There is no material, anatomy, simulation
or save interface change. Wildlife has been told that any future coat follows
the existing body parent in bind coordinates and supplies no independent clock.

No runtime performance conclusion follows from1.198s offline compilation.
The measured populated eye-level GPU p9532.937ms already exceeds16.667ms; no
spare budget is established. The proposed incremental0.20ms GPU/0.05ms CPU
ceilings remain unmeasured. Native coverage, fur identity, motion, allocation and
cost all remain blocked behind an explicitly agreed representation correction.
This closes the one authorized CPU experiment without expanding production scope.
