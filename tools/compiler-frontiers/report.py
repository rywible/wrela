"""Build the durable report from completed experiment artifacts, without rerunning benchmarks."""
from pathlib import Path
import json
import shutil
import statistics

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "output/compiler-frontiers"
DOC = ROOT / "docs/research/compiler-frontiers-2026-09-21"
cpu = json.loads((OUT/"cpu.json").read_text())
appearance = json.loads((OUT/"appearance.json").read_text())
runs = [json.loads((OUT/f"gpu-{i}.json").read_text()) for i in [1, 2, 3]]
manifest = json.loads((OUT/"source-manifest.json").read_text())
for run in runs:
    run["manifest"] = json.loads((Path(run["output"])/"run-manifest.json").read_text())
    assert run["manifest"]["sourceStable"]
assert manifest["sourceStable"]
assert all(not entry["falseExclusions"] for entry in cpu["lighting"])
assert cpu["edits"]["missed"] == 0
assert appearance["weather"]["conservationAbsolute"] < 1e-10
assert appearance["branch"]["events"]["boundaryStress"]["mismatchesOutsideGuard"] == 0


def pct(value):
    return f"{value*100:.3g}%"


def samples(label, mode):
    return [t["ms"] for run in runs for case in run["result"]["lighting"]+[run["result"]["temporal"]]
            if case["label"] == label for t in case["timings"] if t["mode"] == mode]


def median(label, mode):
    return statistics.median(samples(label, mode))


gpu_rows = []
for label, modes in [("lights-mixed", ["generic", "compiled", "specialized"]),
                     ("lights-above", ["generic", "compiled", "specialized"]),
                     ("correlated-filtering", ["analytic", "grid4", "grid8"])]:
    for mode in modes:
        values = samples(label, mode)
        medians = [statistics.median(t["ms"] for case in run["result"]["lighting"]+[run["result"]["temporal"]]
                                    if case["label"] == label for t in case["timings"] if t["mode"] == mode) for run in runs]
        gpu_rows.append(f"| {label} | {mode} | {statistics.median(values):.4f} | {min(medians):.4f}–{max(medians):.4f} |")
gpu_table = "\n".join(gpu_rows)
micro = appearance["microdetail"]["results"]
branch = appearance["branch"]
events = branch["events"]
integral = branch["integratedEvents"]
temporal = cpu["temporal"]
edits = cpu["edits"]
shadow = edits["shadows"][0]
event_stress = events["boundaryStress"]
branch8 = next(row for row in branch["candidates"] if row["shadows"] and row["rank"] == 8)
smooth8 = next(row for row in branch["candidates"] if not row["shadows"] and row["rank"] == 8)
speed4 = median("correlated-filtering", "grid4")/median("correlated-filtering", "analytic")
speed8 = median("correlated-filtering", "grid8")/median("correlated-filtering", "analytic")
errors_gpu = runs[0]["result"]["temporal"]["comparisons"]
for extension, source in [("png", "branch.png"), ("gif", "branch-orbit.gif"), ("weather.png", "weather.png")]:
    shutil.copyfile(OUT/source, Path(f"{DOC}.{extension}"))

report = f"""# Compiler frontiers: six experiments

Research performed on the winter renderer's Apple M4 / Chrome WebGPU environment. These are bounded prototypes; production rendering defaults were not changed.

**Decision:** advance joint procedural integration and explicit visibility events. Develop microgeometry aggregation for broad responses, with a separate treatment for sharp features. Do not build a universal branch-lighting table or deploy compiler light lists on the strength of these probes. Extend existing spatial dependencies when introducing cached transport. Use shared environmental causes as an authoring direction, with explicit artistic controls.

| Opportunity | Measured result | Decision |
| --- | --- | --- |
| Microgeometry → appearance | 8,192 normals → 233 groups: {pct(micro[0]['candidates'][-1]['relativeL2'])} relative L2 for roughness 0.45; sharp roughness 0.08 still {pct(micro[1]['candidates'][-1]['relativeL2'])} error | Useful for broad responses; normal grouping alone does not preserve glints |
| Whole-branch response | Rank 8: {pct(smooth8['heldOut']['relativeL2'])} without self-shadows, {pct(branch8['heldOut']['relativeL2'])} with them; explicit events match {events['comparisons']:,} random comparisons | Compile discontinuities separately from smooth response |
| Eliminate irrelevant lighting | {cpu['lighting'][0]['rejected']}/{cpu['lighting'][0]['evaluated']} region/light pairs removed, identical GPU outputs; modest/variable benefit and no removals for above-ground lights | Not our next large performance investment |
| Integrate motion and correlated effects | Generated finite-Fourier shader: {speed4:.1f}× / {speed8:.1f}× faster than 64/512-sample quadrature in the isolated kernel | Strongest immediate compiler prototype; narrow function class |
| Compile the response to edits | {edits['dirty']}/{edits['patches']} terrain patches rebuilt, {edits['tested']:,} position/normal components match full rebuild; {shadow['outsideLocal']}/{shadow['changes']} shadow changes occur outside the local edit box | Geometry support already exists; transport dependency support is the extension |
| Shared environmental causes | 9,216 cells, six derived channels, temperature controls, mass-accounting residual {appearance['weather']['conservationAbsolute']:.2g} | Useful authoring prototype; no production visual-quality or speed claim |

**1. Detail should sometimes compile to a response, but averaging is not enough.**

The reference integrates 8,192 deterministic surface normals. Snow material is correlated with upward orientation. Clustering uses only normal/material data; 160 view/light pairs are held out from that construction. The shader response is incident-cosine diffuse plus the same GGX form as the engine. There is no geometric visibility or multiple scattering in this probe.

At roughness 0.45, 74 occupied orientation/material groups achieve {pct(micro[0]['candidates'][2]['relativeL2'])} relative L2 error; 233 achieve {pct(micro[0]['candidates'][3]['relativeL2'])}. These reduce **lobe evaluations**, by approximately 111× and 35× respectively. They are not measured frame-time speedups. A single averaged normal/material produces {pct(micro[0]['naive']['relativeL2'])} error in this fixture. At roughness 0.08, even 233 groups produce {pct(micro[1]['candidates'][3]['relativeL2'])}; merely increasing this clustering resolution does not solve narrow glints.

Candidate packed scalar payloads are 1,480 and 4,660 bytes, excluding GPU alignment, metadata and spatial coverage. The current data is one authored distribution, not a validation set of production assets. Statistical response products need coverage, occlusion and temporal tests before they can replace geometry.

**2. Whole-branch compression fails at shadow events; explicit event compilation is much more promising.**

The fixture has 240 procedural flat needle plates, including {branch['snowNeedles']} snow-bearing plates, viewed from three directions. Lighting follows a fixed-elevation azimuth orbit. There are 32 training light angles and 32 interleaved held-out angles. Reference shadowing is at each plate's centroid, not per-pixel visibility. Reflection is diffuse with limited back transmission; there is no trunk, multiple scattering, glossy response or volumetric snow. This is a research diagram, not a finished conifer.

A low-rank response plus angular Fourier interpolation works for the smooth unshadowed case. It fails with self-shadowing: the rank-8 held-out error is {pct(branch8['heldOut']['relativeL2'])}; additional rank does not fix the angular interpolation. Interpolating between two poses also fails on an interior pose ({pct(branch['bends'][0]['poseInterpolation']['relativeL2'])} at bend 0.03), and extrapolation is worse. Endpoint equality is merely a control, not evidence of generalization.

The alternative compiler solves when a ray enters or exits another needle rectangle. For this fixed orbit, boundaries reduce to equations of the form `A cos(theta) + B sin(theta) + C = 0`. It unions the resulting blocked intervals instead of fitting over their jumps.

It emits {events['intervals']:,} intervals: {events['bytes']:,} bytes for float32 endpoints plus offsets, about {events['bytes']/1024:.1f} KiB. Construction took approximately {events['compileMs']/1000:.2f} seconds in this Python reference. The float32 table matched all {events['comparisons']:,} radiance comparisons at 257 independently randomized light angles. Its runtime work is interval lookup rather than an all-needle intersection query; this lookup has **not** been benchmarked in the production GPU renderer.

Boundary stress matters: {event_stress['float32EndpointMismatches']} of {event_stress['rays']:,} deliberately near-boundary queries disagree without handling precision. A measured ±{event_stress['angularGuardRadians']} radian guard catches all observed mismatches and sends {event_stress['fallbackQueries']:,} adversarial queries to the original intersection test. Only {events['randomGuardHits']}/{events['comparisons']:,} random queries touch that guard. This is measured evidence, not a general numerical certificate.

The animation below compares the geometric reference, the failed smooth fit, and the event lookup. All use the same fixed camera and moving light orbit. The still uses the worst held-out light for the fitted model (index {branch['stillLightIndex']}).

![Fixed-branch relighting comparison]({DOC}.gif)

The event boundaries also allow piecewise integration of `visibility × diffuse response`. An antiderivative table for a uniform **angular line emitter** of width {integral['angularWidth']} radians produces {pct(integral['compiled']['relativeL2'])} error against 256-sample geometric quadrature, versus {pct(integral['centerSample']['relativeL2'])} for one central light sample. The 128→256 reference convergence difference is {pct(integral['referenceConvergence']['relativeL2'])}; the reported residual therefore includes reference quadrature error. This is not integration over the two-dimensional solar disk. The current float64 integral product is {integral['float64Bytes']:,} bytes, substantially larger than the visibility-only table.

**Limits:** fixed geometry and a one-dimensional light path are essential to this result. Deformation, changed snow geometry, different light elevation and nearby blockers invalidate it. Per-needle centroid visibility is insufficient for close-up quality. Compilation considers needle pairs; unique shape variants and pose tables can cause prohibitive compilation and memory growth. Identical families can share products, but arbitrary procedural variation cannot safely share their visibility.

**3. Correctly deleting work does not guarantee a faster GPU program.**

The compiler constructs position balls and normal cones for 256 patches generated by the actual winter terrain compiler. It rejects a light only if every admitted normal points away from every admitted light direction. It disables this rule for transmission/two-sided materials and widens the cone for known motion. The production point-light attenuation has infinite support, so no invented distance cutoff is used.

The intentionally favorable mixed fixture puts four of eight lights below the terrain. It excludes {cpu['lighting'][0]['rejected']}/{cpu['lighting'][0]['evaluated']} pairs ({pct(cpu['lighting'][0]['rejected']/cpu['lighting'][0]['evaluated'])}) with zero false exclusions across {cpu['lighting'][0]['tested']:,} sample/light checks. All eight lights above the terrain exclude zero pairs. These bounds require the actual shading normals, including bump and deformation; unbounded normal perturbations must keep the general path.

GPU tests reuse the production GGX function and compare a generic eight-light loop, a compiled compact list, and generated programs grouped by identical active-light sets. All outputs match exactly. The mixed case requires 13 generated variants; the above-ground case requires one. Lists show a modest warmed benefit in the favorable case and a clear penalty where nothing can be removed. Extra dispatches/indirection often erase the theoretical saving. The generic shader already cheaply returns zero for back-facing GGX, which limits the available benefit.

These tests do not establish a substantial frame-time win. A sensible future selector must retain the generic program when specialization loses; many more lights would require a separate architectural investigation.

**4. Joint integration is a real compiler transformation, not just a handwritten shortcut.**

`fourier.ts` accepts a small expression tree of constants, integer-carrier cosines, addition and multiplication. It expands products by convolution, combines equal modes, preserves correlations and emits WGSL. Unsupported functions are outside this prototype; excessive symbolic expansion rejects the transform instead of truncating it. It uses the existing compiler phase-footprint machinery on the CPU.

The test expression multiplies two correlated signals: `(0.6 + 0.35 cos(phi)) × (0.5 + 0.45 cos(phi + delta))`. Compilation yields a constant and four conjugate mode pairs. Pixel and shutter integration are analytic for an affine phase footprint; the source expression itself has no fitted approximation.

Across 160 independent footprints, the compiled result differs from 48³ dense samples by {pct(temporal['compiled']['relativeL2'])} relative L2. Independently filtering the factors gives {pct(temporal['independentFactors']['relativeL2'])}; a point sample gives {pct(temporal['pointSample']['relativeL2'])}. The 24³→48³ convergence difference is {pct(temporal['referenceConvergence']['relativeL2'])}. The correct joint average retains energy that independent filtering deletes.

The generated GPU program was checked at 65,536 inputs against an independent CPU closed-form expression; maximum absolute error is {runs[0]['result']['temporal']['cpuReference']['maximum']:.3g}. Its isolated median is {median('correlated-filtering', 'analytic'):.4f} ms, compared with {median('correlated-filtering', 'grid4'):.4f} ms for 64 samples and {median('correlated-filtering', 'grid8'):.4f} ms for 512. Those sampled GPU alternatives still have {pct(errors_gpu[0]['relativeL2'])} and {pct(errors_gpu[1]['relativeL2'])} error versus the analytic result.

This does **not** imply those speedups for water or the whole frame. GGX, arbitrary noise warps, thresholds, visibility and nonlinear motion do not generally belong to this finite function class. Deliberately applying an affine footprint to accelerated phase motion gives {pct(temporal['curvedMotionFailure']['relativeL2'])} error. The compiler must carry validity conditions or subdivide/fall back when the phase model fails.

**5. Spatial edit compilation already exists in part; transport is the missing relationship.**

`packages/world/src/session.ts` already filters terrain interventions into each patch's content key with a spatial margin. This experiment does not claim a newly invented terrain cache or an added production performance win.

The probe adds a local radius-1.2 m, height-4 m terrain intervention. Including the existing 5 cm finite-difference normal stencil selects {edits['dirty']} of {edits['patches']} fixed patches. Rebuilding those and reusing the others exactly matches full regeneration across {edits['tested']:,} float32 position/normal components. Timing is retained in the raw artifact, but rebuild counts and exact equality are the reliable evidence here.

The shadow counterexample uses a separate sampled height-field horizon reference, with the sun along +x. At elevation 0.12 radians, {shadow['changes']} receivers change shadow state, {shadow['outsideLocal']} outside the local edit box. A conservative downstream strip invalidates {shadow['conservativeInvalidated']:,}/{shadow['samples']:,} receivers and misses none. Elevation 0.6 is also tested. This establishes why future compiled lighting products need transport-aware influence regions. Reflections, indirect illumination and changed water paths can require much broader invalidation than this one-direction shadow example.

**6. Shared causes can replace independent authoring channels, but the physical model still matters.**

A 96×96 toy terrain runs snowfall interception, conservative directional drift, temperature-controlled melt, downhill liquid routing, retained wetness and a later refreeze. Snow coverage, albedo, roughness, wetness, ice and an illustrative branch-load response derive from the same state. Cold and warm controls change melt and ice consistently; water-equivalent mass plus boundary export is conserved to floating-point precision.

![Shared environmental fields, not a production scene]({DOC}.weather.png)

This proves a dependency/coherence pattern, not realistic snow. Routing is grid-directed, canopy exposure and thermal coefficients are illustrative, and branch stiffness is an authored parameter. The visible grid artifacts are evidence that better transport/discretization is needed for an art-quality implementation. Artist overrides and deliberate exceptions remain necessary. No measured authoring-hours saving or rendering speedup is claimed.

**What I would build next**

1. Promote the finite expression/integration prototype into a narrowly supported appearance product, then validate an actual rendered procedural material across camera distance and motion. Retain joint carrier identities and generated shader source. Do not force arbitrary shaders into this representation.
2. Extend the branch experiment to spatially varying visibility and a small, explicitly supported deformation domain. Compare explicit events against a proper geometric reference and a broad-response aggregate. Test memory and recompilation cost across many unique branches before adopting it for forests.
3. Add transport influence/dependency metadata when a compiled lighting product first needs it, reusing existing product invalidation and terrain support machinery. Keep sharp events separate from smoothly approximated response.
4. Use a shared snow/wetness state when authoring the richer winter scene; treat it as a controllable content generator and validate the result visually.

The architectural implication is a small set of different realizations selected from authored structure: finite analytic integrals where algebra closes, explicit events where visibility changes, statistical aggregates where only the broad response survives, and ordinary rendering elsewhere. A universal cached response would hide exactly the difficult cases this investigation exposed.

**GPU methodology and evidence**

Three independent hardware Chrome runs, each with eight alternating-order trials per mode. Each timestamp spans 32 dispatches; results below divide by 32. The lighting workload evaluates 262,144 queries by repeating 16,384 terrain points 16 times. The integration workload evaluates 65,536 footprints. These are offscreen compute throughput tests, not scene frame times, display FPS or end-to-end game performance. Adapter clocks were not measured; run medians expose variation. No timings from overlapping renderer passes are added together.

| Workload | Program | Pooled median ms/query batch | Range of the three run medians |
| --- | --- | ---: | ---: |
{gpu_table}

All three final browser manifests report `sourceStable: true`; their bundles and raw results are preserved at:

{chr(10).join('- '+run['output'] for run in runs)}

The CPU manifest is stable, with source fingerprint `{manifest['sourceFingerprint']}`. The appearance implementation SHA-256 is `{appearance['sourceSha256']}`. Raw CPU/appearance data, emitted shadow-event arrays and references, and per-run GPU JSON live in `{OUT}`. The consolidated companion JSON preserves the numerical evidence.

Validation: 27 TypeScript tests passed (research plus existing phase and streaming tests), 5 Python invariant tests passed, both TypeScript configurations passed, workspace boundaries passed, and formatting checks passed for the six new TypeScript files. GPU runs reported no validation errors, exact lighting equality, and generated-integral agreement with the CPU reference. Production rendering was not modified, so no new winter scene frame-time improvement is claimed.

**Prior art and what is specific to Wrela**

Appearance-preserving geometry aggregation has substantial precedent: [Loubet and Neyret's hybrid mesh/volume LoDs](https://onlinelibrary.wiley.com/doi/abs/10.1111/cgf.13138) separate resolved surfaces from sub-resolution structure. [Yang and Barnes](https://yyuting.github.io/docs/eg_2018.html) demonstrate compiler-based smoothing of procedural programs. These precedents support investigating automatic transformations; they do not establish that our prototypes are equivalent or production-ready.

Visibility-event representations also predate this work; [Seales and Dyer's shaded polyhedral animation](https://graphicsinterface.org/wp-content/uploads/gi1990-21.pdf) exploits changes in visibility, while [coherent shadow maps](https://diglib.eg.org/server/api/core/bitstreams/1fb3a412-4a6a-496e-9140-c6592dd10252/content) compress precomputed visibility. [Fearing's snow model](https://www.cs.ubc.ca/labs/imager/tr/2000/fearing2000a/) treats accumulation and stability as causes of appearance.

Our opportunity is to derive suitable representations, validity conditions and dependencies directly from semantic authoring, with reproducible comparisons and automatic fallback. No claim of scientific novelty or exclusivity is made.
"""
Path(f"{DOC}.md").write_text(report)
Path(f"{DOC}.json").write_text(json.dumps(dict(cpu=cpu, appearance=appearance, gpu=runs,
    cpuSourceFingerprint=manifest["sourceFingerprint"], productionRenderingChanged=False), indent=2))
print(DOC.with_suffix(".md"))
