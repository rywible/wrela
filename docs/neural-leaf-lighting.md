# Nonlinear leaf lighting: retire the uniform radiance cache

2026-10-28 local / 2026-10-29 UTC. Parent421f740ce2f5c7aa4acf6ebe26ffba572fef4cc0.
**Decision: change the representation.** A uniform cache trades visible high-frequency
lighting error for speed. Cubic filtering improves quality but consumes that speed
advantage. Direct evaluation of the authored source is the strongest measured control
for this workload. This block establishes no new joint rendering win and no G1–G7 pass.

First complete quality/runtime/residency assessment: **23.10 minutes** from the recorded
start. Decision: **29.65 minutes**. One targeted optimization, zero training updates.
Failed results remain in the evidence. All work is solo on Pop!_OS; the Mac only
orchestrates SSH. Goals and previous models remain unchanged.

## Consequential question and fair inputs

The existing74cases use constant, affine or bilinear radiance already solved by the
[direct source control](neural-coverage-radiance.md). They cannot decide whether a
radiance cache/model helps nonlinear production shading. Four new public diagnostic
cases use the actual engine `leaf_response`, authored spatial normal/optical-depth
fields, moving layered geometry and current lighting:

| Case | Source shading | Event |
|---|---|---|
|74|Lower spatial frequency|Moving geometry and slowly rotating sunlight|
|75|Higher spatial frequency|Moving geometry and slowly rotating sunlight|
|76|Lower spatial frequency|Illumination drops to35% at frame16|
|77|Higher spatial frequency|Illumination drops to35% at frame16|

These are analytic layers using a production material response, **not a production3D
foliage pass, GI or shadow reference**. Four public cases are not sealed generalization.
Geometry, source code, material regions, current lights and arbitrary source queries
are available to both caches and direct shading. All source calls count, including
occluded material coordinates. The ordinary temporal decoder is a secondary control;
the equally informed direct source decoder determines the architecture decision.

The cache stores three separate material lattices. At4× it uses at most one sixteenth
of output-pixel count in source-shading calls:1,248 at192×108,129,600 at1080p,518,400
at4K. Each node stores f32RGB plus padding. Every frame fully refreshes all nodes;
there is no learned model, stale history, rendered truth input or hidden fallback.
The reconstruction integrates independently clipped material areas and samples each
material at its visible-area centroid. Direct source shading uses those same regions
and evaluates the actual shading program at each centroid. It is not constrained to
one-sixteenth shading calls. Its radiance integration is an approximation, not truth.

Bilinear uses four lattice loads per material. The sole followup uses tensor-product
Catmull–Rom cubic interpolation with sixteen loads, clamped boundary indices and
nonnegative final RGB, at unchanged shading-call and resident-storage budgets.
This is our implementation of the published interpolation idea; no code or weights
were copied. [Catmull and Rom,1974](https://doi.org/10.1016/B978-0-12-079050-0.50020-5).

## Reference and quality

Reuse the existing sequence capture, FLIP, LPIPS and temporal scorers. The new
reference combines independent analytic material areas with midpoint shading
quadrature. Initial4×4 vs8×8 failed the predeclared mean-view-error tolerance on
case77:0.000107424>0.0001. That failure is preserved. Refined8×8 vs16×16 passes:
worst mean0.0000167566, worst p99 0.000136433, versus limits0.0001/0.001.
Maximum single-channel error is0.00137004; this convergence test is not a
continuum error proof.

Six frames per case, including the illumination discontinuity, were compared at all
four scales. All512 original direct outputs are unchanged by reference refinement.
Candidate RGB captures remain immutable; separate metric directories pair them with
the improved targets. One independent material-area test agrees with64×64 point
quadrature on dyadic subrectangles. No reference machinery beyond the demonstrated
convergence uncertainty was expanded.

Quality uses32frames per case, scales1–4 at192×108. Named view:
max(RGB,0)/(1+max(RGB,0)), exposure1; LDR-FLIP67pixels/degree,
LPIPSAlexNetv0.1, PSNR on sRGB-encoded named view. Primary4× results:

|Case|Direct FLIP|Bilinear FLIP|Cubic FLIP|Temporal FLIP|
|---|---:|---:|---:|---:|
|74|0.002021|0.010110|0.006352|0.149225|
|75|0.003937|0.094793|0.070270|0.162707|
|76|0.001958|0.010017|0.005945|0.252707|
|77|0.002953|0.082990|0.063294|0.190099|


|Method|PSNR range dB|LPIPS range|Opaque temporal residual range|Newly exposed error range|
|---|---:|---:|---:|---:|
|Direct source|63.2719–73.2077|1.98482e-06–1.80361e-05|3.23015e-05–4.27335e-05|5.92828e-05–0.000269968|
|Bilinear cache|32.7286–56.3763|0.000168002–0.0336761|5.28201e-05–0.00043963|0.000668687–0.0148452|
|Cubic cache|35.1603–58.5587|6.40014e-05–0.0191243|4.8561e-05–0.000350791|0.00038297–0.0107334|
|Ordinary temporal|19.5929–22.7161|0.281379–0.653751|0.00295935–0.011733|0.0454813–0.0555766|


The1×/2×/3×/4× maximum mean FLIP of the bilinear cache is
0.01187/0.03512/0.06380/0.09479. A smooth-case result does not qualify the sharp cases.
Cubic lowers4× mean FLIP by23.7–40.6% versus bilinear, but its sharp cases still
score0.07027/0.06329. Direct scores0.00196–0.00394 across all cases.

Silhouette quality is scored as shaded RGB error in a one-output-pixel band around
reference foreground-area changes. This is not alpha equality, strand precision/recall
or the full G3 gate. Worst clip-mean band error at4×:
 Direct source=0.000260581; Bilinear cache=0.0144359; Cubic cache=0.0105405; Ordinary temporal=0.0701482; all values are in the named linear view.
All2,048 scored outputs are finite with alpha1. Detailed frame, edge p99, visibility
change and newly exposed errors are retained; none are discarded from the decision.
No blinded study, full-resolution quality qualification or broad G3 claim was run.

## Complete measured cost

AMD Ryzen5 5500U, RADV RENOIR PCI1002:164c, Mesa26.1.6,
Chrome154.0.8037.57, Wrela0.9.0/compilerde9b81e52a7747870dfbd5223cb8aeb41e584d23.
One quiet GPU queue; no scoring/training/build job overlaps timed GPU runs.
No enforced thermal/power lock or twenty-minute sustained qualification.

120measured frames after60warmup, one worker,4×. GPUspan includes all cache shading
and refresh, exact-source visibility/material integration, decoding and final
diagnostic composite. There is no sort, wood, shadow or GI work in this fixture;
these are absent, not costs excluded from a G4 claim. No reference computation is
timed. Tables report initial unpaired runs, not confidence-qualified small differences.
The paced p95/p99 come from a separate timestamp stream in each profile build.

|Method|Case|Output|Saturated GPU median ms|Paced GPU p95 / p99 ms|
|---|---:|---|---:|---:|
|Direct source|74|1920×1080|2.25112|3.18066 / 3.19679|
|Direct source|74|3840×2160|8.86216|9.68985 / 9.76380|
|Direct source|75|1920×1080|3.29396|4.16856 / 4.26487|
|Direct source|75|3840×2160|13.08708|13.22434 / 13.30650|
|Bilinear cache|74|1920×1080|1.44356|2.39163 / 2.46828|
|Bilinear cache|74|3840×2160|5.65384|6.52364 / 6.63536|
|Bilinear cache|75|1920×1080|1.95304|2.39233 / 2.87661|
|Bilinear cache|75|3840×2160|7.61416|8.48870 / 8.63720|
|Cubic cache|74|1920×1080|2.31348|3.21468 / 3.23965|
|Cubic cache|74|3840×2160|9.10400|9.82700 / 9.99067|
|Cubic cache|75|1920×1080|3.32324|4.23481 / 4.32482|
|Cubic cache|75|3840×2160|13.22728|13.40061 / 13.56998|
|Ordinary temporal|74|1920×1080|6.73136|7.54603 / 7.63429|
|Ordinary temporal|74|3840×2160|26.41540|26.62667 / 26.85279|
|Ordinary temporal|75|1920×1080|6.86628|7.65348 / 7.72555|
|Ordinary temporal|75|3840×2160|26.90268|27.19288 / 27.43063|


Bilinear is36–42% cheaper in these saturated runs, with materially worse quality.
Cubic costs roughly direct shading and is substantially less accurate; a1–3% timing
difference is not treated as statistically established. Source preparation and
lighting updates are already included because every cache is regenerated every frame.
Host-call distributions and paced invocation intervals are in decision.json.
CPU-to-GPU completion and physical presentation were not measured in this block.
Quality was measured at192×108; timing at1080p/4K is a separate scaling measurement,
not matched full-resolution quality validation.

## Resident memory

All requested allocations include output, scratch, inactive fixture buffers, weights,
caches and histories. Zero hidden fallback/learned allocation. Live/peak are requested
bytes; DRM/PSS are actual deduplicated whole-client/process snapshots after300paced
frames, not physical peaks or per-asset allocations. Do not sum UMA memory pools.
Decimal MB below; DRM/PSS ranges span cases74/75.

|Method|Output width|Requested live / peak MB|DRM VRAM MB|DRM GTT MB|Browser PSS MB|
|---|---:|---:|---:|---:|---:|
|Direct source|1920|25.21206 / 33.50646|60.93–69.85|18.43–18.43|594.96–597.09|
|Direct source|3840|99.86166 / 133.03926|185.25–218.81|43.31–43.31|672.52–694.96|
|Bilinear cache|1920|27.28564 / 35.58004|69.80–69.87|18.43–18.43|595.16–595.45|
|Bilinear cache|3840|108.15604 / 141.33364|227.13–227.13|43.31–43.31|671.08–671.91|
|Cubic cache|1920|27.28564 / 35.58004|69.87–69.87|18.43–18.43|550.66–551.10|
|Cubic cache|3840|108.15604 / 141.33364|227.13–227.13|43.31–43.31|672.32–672.75|
|Ordinary temporal|1920|48.02160 / 56.31600|95.13–95.13|18.43–18.43|595.69–597.63|
|Ordinary temporal|3840|191.10000 / 224.27760|284.82–284.82|43.31–43.31|690.14–692.82|


The cache adds2,073,584 /8,294,384 live requested bytes over the direct fixture at
1080p/4K (the direct fixture has a16B dummy cache buffer in these measured builds).
No actual resident-memory advantage over direct shading is established. The lower
1080p cubic browser PSS is process/runtime variation, not a smaller radiance cache.
This is not the G4 appearance-representation comparison or its16× claim.

## Retention, failures and next bet

Retain four nonlinear source cases, the converged8×8 reference, its one geometry
check, and the equally informed direct-shading control. Remove both experimental
cache decoders and their allocations. The engine/content default renderer is unchanged.
264 compared image/target/visibility files are byte-identical, including old cases72/73,
new direct/default cases and an independently restarted camera-cut target.
Checks are limited to the changed tool/fixture and this retained behavior.

Preserve initial reference failure, bilinear/cubic sources and all quality/timing
results. A pre-run cubic syntax error was fixed; the first cleanup script failed
before changing source because it expected a different pass-end spelling. The repair
and logs are archived. Initial host metadata accidentally labelled absolute invocation
timestamps as completion statistics; the original metadata is retained, and the final
record correctly reports host calls/intervals with completion explicitly unavailable.
No failed rendered result is deleted.

Next architecture bet: choose rendering approximation from authored program structure.
Integrate cheap terms exactly, evaluate sharp cheap terms directly, and reserve
samples/model capacity for expensive smooth residuals with a measured fallback.
First establish a workload whose source-shading cost justifies that residual model.
Do not train a model merely to repair this fixed sparse lattice. The simpler cache
has a smooth-case speed/quality tradeoff, but that is insufficient for the declared
cross-case objective.

Evidence: `build/neural-evidence/surface-leaf-lighting-v1` on Pop!_OS. Authoritative
per-variant source archives, executable profile builds, original captures and frozen
dependency manifests are sealed. See replay.md in that directory. Manifest:
`202f6416ca1abebcc330959b565270597a01ffdf284e3c220a15aeeeaf8e461b`.
