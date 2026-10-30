# Refined spheres replace the source-specific visibility envelope

2026-10-30. **Decision: change representation back to the original spheres and
retain the finer HZB query.** Remove the source-view table, wind envelope and
production wind arguments introduced by the previous block. Against that
source-envelope parent, normal-route total GPU changes
**-0.097% to +0.252%** across 4 matched comparisons.
Requested live allocation falls **137,280 bytes** and all **36 shaded checks pass**.
This is a simplification with closely matched measured GPU cost; no statistically
established GPU speedup over the source-envelope parent is claimed.

Against the original coarse-sphere renderer, 1 matched normal-route
direct comparisons show total GPU changes **-1.398%**.
The finer query recovers the useful conventional improvement without requiring
the extra source metadata on these routes. This revises the attribution of
[the previous block](neural-impostor-source-cull.md); it does not refute the
broader usefulness of source information.

First joint quality/runtime/residency assessment: **23.54 minutes**.
Decision: **35.65 minutes**. **Zero optimization followups.**
Repeats use unchanged executables to resolve demonstrated source mismatches.
Solo Pop!_OS AMD; frozen models, zero training, unchanged and unmet G1–G7.

## Fair controls and implementation

Parent: 82cbaf583e9b26659b932bcdd4913426eac312f1, the source envelope plus finer query.
Coarse control: original sphere path from 1ef44ec6b86f800bdc837591a527a42ab21fd992.
Candidate: that original sphere path with optional finer HZB queries.
All have identical useful source fields, instances, geometry, materials, wind,
visibility history, sampling, lighting and reconstruction. No arm gets reference
radiance or privileged learned inputs. The source table's removal is the point
of the ablation, not a restriction on information available to the control.

The initial coarse query remains exact. Only when it cannot establish occlusion,
and only for the same pure flat-impostor LODs eligible in the source-envelope
parent, the candidate queries the same screen rectangle two mip levels finer.
It stops on the first non-occluding cell. Outward pixel/depth margins and near-
plane handling preserve openings; same-frame second-phase reveal remains active.
Mixed, near and volume LODs and other cull callers retain the original behavior.
The geometric query is conservative conditional on a valid supplied sphere.
The inherited plant reach/radius has no explicit wind expansion, so these results
qualify the current production wind (strength 0.035) and captured views, not
arbitrary deformation or stronger wind. The synthetic query test proves handling
of supplied bounds; it does not prove every animated source stays inside them.
Broader source-bound correctness remains part of the unmet G3 qualification.

[engine/cull.wrela](../engine/cull.wrela) adds a default-false refine parameter.
[engine/wood.wrela](../engine/wood.wrela) uses the existing LOD and slot visibility
to select it. The packed table, its update dispatch, the billboard/wind envelope,
and the two production stages' extra wind plumbing are removed.
Original LOD buffers, source atlases, caches and fallback resources stay counted.

This is conventional hierarchical Z querying, building on the existing methods
in [citations.md](citations.md) (Greene, Kass and Miller; Haar and Aaltonen).
No algorithmic novelty, unique Wrela advantage or G4 memory breakthrough is claimed.

## Complete AMD cost and residency

Ryzen 5 5500U / RADV RENOIR PCI 1002:164c, Mesa 26.1.6, Chrome 154.0.8037.57,
kernel 7.1.5-76070105, Wrela 0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. No thermal/power lock.

Six prebuilt executables: three arms for each of two trajectories. Each run is
660 frames, 1920×1080 output, 0.667 internal scale, one worker, pacing enabled
and two frames in flight. **21 runs and 147 periodic captures**, seven per
executable run, with a quiet serial GPU queue. Builds, scoring and toolchain
queries do not overlap timing or reference capture.

The normal route is the established controlled World/time/camera snapshot with
background simulation continuing: rise 96 m at frames 240–299, return, move 96 m
during 360–539, then settle. The elevated route keeps the eye 96 m higher for
the entire run, aimed over the forest toward the horizon. It tests another
visibility distribution with less near overlap. It is not an empty scene and
no measured occlusion fraction or general low-occlusion guarantee is claimed.

Initial order for each route: parent/coarse/spheres, then spheres/coarse/parent.
Normal-route replacement rounds 3–4 repeat that alternating order; final round 5
is coarse/spheres/parent. These are fresh browser runs of unchanged executables.
Source matching is decided from camera, player and source-census equality;
mismatches are preserved and excluded, never presented as wins.

| View, round | Control → candidate | Total GPU delta | Movement GPU delta | Browser route delta, ms | CPU movement p95, ms |
| --- | --- | ---: | ---: | ---: | ---: |
| route, 1 | parent → spheres | -0.004% | -0.017% | -121.960 | 146.940 → 140.187 |
| route, 3 | parent → spheres | +0.029% | -0.805% | -51.880 | 145.682 → 141.390 |
| route, 4 | parent → spheres | +0.252% | +1.292% | -54.870 | 143.422 → 144.937 |
| route, 4 | coarse → spheres | -1.398% | -2.310% | -554.325 | 148.360 → 144.937 |
| route, 5 | parent → spheres | -0.097% | +0.422% | -173.505 | 140.047 → 141.305 |
| open, 1 | parent → spheres | +0.418% | -0.176% | +329.695 | 138.150 → 139.646 |
| open, 1 | coarse → spheres | -0.450% | -0.290% | -260.330 | 141.718 → 139.646 |
| open, 2 | parent → spheres | -0.141% | +0.936% | -63.010 | 144.535 → 147.250 |
| open, 2 | coarse → spheres | -0.321% | +1.016% | +60.750 | 145.003 → 147.250 |

GPU cost is the complete frame span, including startup, source arrivals/updates,
visibility, wood, decoding, sorting where present, fields, lighting, temporal
reconstruction, compositing and submission gaps. CPU and browser metrics remain
separate. Browser elapsed and CPU tails do not improve consistently across views:
this is not a general latency win. The table exposes both signs rather than
aggregating the elevated view away. All per-window p50/p95/p99/maxima and labels
are in the raw assessment. Normal-route settled frames remain about 36.5 ms,
far above G6's 8.33 ms target.

The source-envelope parent remains 1.67–1.98% faster than the coarse control in
the initial/replacement matched rounds 2 and 4, corroborating the earlier block.
This block isolates a cheaper way to retain nearly the same route benefit.
Controls are shared within each triplet; direct coarse/sphere rows reuse the
same runs and are not additional independent samples. No confidence interval
or claim of a universal speedup is inferred from these few repeated routes.

| View, round, candidate versus source-envelope parent | Requested live delta, B | Final DRM VRAM delta, B | Final process-group PSS delta, B |
| --- | ---: | ---: | ---: |
| route, 1 | -137,280 | +8,904,704 | -49,083,392 |
| route, 3 | -137,280 | -24,616,960 | -47,516,672 |
| route, 4 | -137,280 | -630,784 | -4,256,768 |
| route, 5 | -137,280 | +8,925,184 | +1,069,056 |
| open, 1 | -137,280 | -12,288 | -43,038,720 |
| open, 2 | -137,280 | +36,864 | +4,258,816 |

| Observation across matched source-envelope/sphere runs, bytes | Source-envelope parent | Refined spheres |
| --- | ---: | ---: |
| Requested live | 707,993,416–708,009,172 | 707,856,136–707,871,892 |
| Requested peak | 895,189,532–914,557,784 | 892,388,136–907,751,252 |
| Final DRM VRAM | 904,830,976–945,225,728 | 904,818,688–928,997,376 |
| Final DRM GTT | 22,544,384–23,019,520 | 22,626,304–22,855,680 |
| Final process-group PSS | 1,077,142,528–1,098,421,248 | 1,034,103,808–1,094,563,840 |
| Sampled maximum DRM VRAM | 1,056,575,488–1,103,040,512 | 1,065,312,256–1,083,932,672 |
| Sampled maximum DRM GTT | 22,585,344–23,052,288 | 22,642,688–22,896,640 |
| Sampled maximum process-group PSS | 4,254,625,792–4,314,674,176 | 4,233,648,128–4,302,981,120 |

The requested-live reduction is exact for matched sources: 33 kinds ×
260 vec4 entries × 16 bytes = 137,280 bytes. It is a small metadata reduction,
not G4's 16× appearance-representation compression. Physical VRAM/PSS deltas
are mixed; no consistent physical-memory reduction is claimed. All scratch,
caches and original fallback data are included.

DRM VRAM/GTT and process-group PSS overlap on this UMA device; never sum them.
Physical samples request 50 ms spacing and actual gaps are recorded. Sampled
maxima are not true physical peaks. Explicit author edits, independent moving
objects and measured display latency are not separately qualified here.

## Shaded quality, silhouettes and exclusions

The candidate reuses the completed focused 192×108 forced-impostor and native
1920×1080 normal-LOD references, four temporal scales and three quadrature
controls. Whole-image/crown-region scores give **36/36 passes**.
Worst FLIP ratio **1.042755837**, LPIPS ratio **1.023757834**,
PSNR change **-0.222027 dB**, with unchanged 1.05/1.05/−0.25 dB gates.
These include previous renderer differences from older completed references,
not just this block's change. Candidate integrated RGB is never truth.

Native raw leaf-class coverage precision is **0.999993027204**, recall
**1.0**, symmetric boundary p99 **0.0 pixels**
and maximum **2.828427 pixels**.
Union MAE is 0.000022278467; 214 pixels exceed 2/255 and 28 exceed
0.1 coverage error. The class includes impostor wood and excludes near wood.
Exposure 1, Reinhard, display sRGB, AlexNet LPIPS v0.1 and LDR FLIP 67 pixels/degree,
no resizing. These are established development gates, not independent transport
truth or the full G3 strand/branch/motion tests.

Of 21 comparisons, **13 match** and **8 are excluded**.
That comprises 10 matched comparisons against the source-envelope
parent and 3 matched direct coarse/refined comparisons.
The exclusions and every differing census record are saved in qualified-assessment.json.
The known initial/replacement source mismatches are coarse round 1, spheres
round 2 and coarse rounds 3 and 5: census record 9 has 5,907 capsules versus zero in
the corresponding parent. The last repeat therefore did not supply a second
valid direct coarse/refined normal-route comparison. The four matched
source-envelope/refined-sphere comparisons support the simplification decision;
a repeated direct speedup against coarse spheres remains unproven.
Their requested-memory and image differences are
not improvements. Both runtime and failed source-matching results remain
available. Matched normal-route paired RGB channel differences are at most
1/255; elevated-route differences are at most 2/255.

## Verification and next architecture bet

The focused GPU fixture tests ten independent wall/opening/front/near-plane/
disabled/offscreen/equal-depth/odd-size cases. Its decisive outside-gap case
compares coarse and refined queries on the exact same source. Deliberately
disabling the finer path makes that case fail; the mutation and failure are
preserved, restoration is exact, and the final fixture passes.
Existing cull and wood-merge tests also pass. Only changed files are formatted
and the established allow check runs. The obsolete wind-envelope fixture is
replaced; its original sources and tests remain in the previous sealed evidence.

**Retain refined spheres and retire the per-view source envelope/table for this
path.** Do not spend another optimization on this small metadata tradeoff.
Quality holds, GPU route cost is close, representation and plumbing are simpler,
and requested memory falls; CPU/physical-memory limits remain explicit.

The next larger hypothesis is **direct compiled evaluators for known nonempty
field families**, with the generic tape interpreter retained as a measured
fallback. [shape_tape](../games/last-green/blockouts.wrela) currently lowers
known stone, slab, block, waystone, wall and rock families into generic programs;
[the distance renderer](../engine/distance.wrela) repeatedly interprets them.
Preserving their source family and parameters may avoid interpreter workspace
and dispatch overhead while retaining exact field semantics. Compare against
the current compact/slab-specialized interpreter, give both the same source
knowledge, and charge routing, updates, fallback data and shader/cache residency.
First validate field values and rendered quality, then complete runtime/memory.
This is unimplemented; no saving is claimed.

Evidence on Pop!_OS: build/neural-evidence/sphere-refinement-v1.
Manifest SHA-256: 8b212d39eadfaf25cf4e766d86a63bc2c0b9e05e2b87a65c494268ca61d73307. Exact variants, builds, all 21 raw runs and 147
route PNGs, scores, memory, references/dependency hashes, frozen artifacts, source
archives, replay scripts and test failure are preserved. The inherited unrelated
benchmark diff remains unstaged.
