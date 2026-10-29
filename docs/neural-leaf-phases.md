# Leaf coverage phases and exact constant regions

2026-10-29. Parent `9bc0239e93def0f0307b614f9202c3bd5cdb2d18`. **Retire.**
Retire both two-sample variants for native LPIPS failure. The constant-region variant preserves all four scored reconstruction images exactly and keeps requested allocation unchanged, but its five-pair timing comparison does not establish a complete-frame improvement. Restore parent and preserve all failures.

First complete shaded-quality/runtime/residency assessment: **16.95 minutes**.
Decision: **46.52 minutes**, after exactly two optimization followups.
G1–G7 and the eight frozen artifacts remain unchanged; no neural training occurred.

## The consequential comparison

The [card geometry](../engine/vegetation.wrela) uses camera right/up at one depth;
[card rotation](../engine/plant.wrela) is a pure in-plane rotation. At correct aspect,
its local screen derivatives are equal-length and orthogonal, up to arithmetic.
A covariance-anisotropy correction would largely repeat the previously rejected
scalar mip filter. The original four actual sample locations nevertheless retain
a rotated-square orientation. This source finding changed the proposed experiment
before implementation; no unexecuted directional speedup is claimed.

The control is the strongest retained conventional path: original four-sample
coverage, tight source outlines, and the packed proven-empty certificate.
Every candidate receives the same source atlas/code, visibility, leaf IDs, camera,
wind and sampling schedule. No privileged reference or future frame is available.

|Trial|Question|Outcome|
|---|---|---|
|Initial phased pair|Can alternating opposite pairs replace four samples?|Native LPIPS +16.2%, 2× +8.8%; rejected.|
|Followup 1 fixed pair|Does eliminating pair changes repair the regression?|Native LPIPS +10.9%; rejected.|
|Followup 2 constant regions|Can source proofs skip reads while retaining their exact result?|All four reconstruction outputs exact; six of seven HDR buffers exact; joint cost/residency and paired timing decide retention.|

The phased variant chooses the original 0/3 or 1/2 diagonal, each read weighted 0.5.
The selector mixes frame bits and high seed bits; both diagonals occur equally for
each relevant low phase in an aligned 64-frame cycle. This preserves a linear
quadrature average, not nonlinear alpha tests, depth, composition or temporal history.
The fixed variant changes only the selector to a card-stable high seed bit.
Both keep unconditional centre/derivatives, sharp coverage, leaf admission and the
same all-four empty certificate. Shadows remain unchanged.

The final variant restores all four original positions and weights. It packs a
second conservative distance into the existing support byte: five bits retain the
original empty bound exactly, and three formerly unused bits store distance to
any nonopaque texel or different green/leaf-ID value. That second distance is
capped at seven texels and rounded down. A strict footprint test, including bilinear
support and a boundary margin, certifies constant opaque coverage. Only then is the
area sum known from the existing centre leaf ID and per-card admission. Unknown
regions retain all four reads. See constant-certificate.md in the evidence for the
bound argument. The immutable 512-square level-zero atlas contract remains explicit;
default callers still use the ordinary path. No new texture, model or history exists.

## Complete cost and resident memory

AMD RADV RENOIR PCI1002:164c / Ryzen 5 5500U / Mesa 26.1.6 / Chrome 154.0.8037.57.
Wrela 0.9.0 compiler `de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
Full clearing 1920×1080, production 2× reconstruction, one worker, 600 warmup and
60 measured frames, fixed workload with pacing/adaptation disabled. Separate paced
timing and allocation replays use the same executable. No thermal/power lock.

|Measurement|Baseline|Phased|Fixed|Constant|
|---|---:|---:|---:|---:|
|Complete GPU median, ms|24.99248|24.46324|24.56072|24.82260|
|Complete GPU p95, ms|26.60340|26.08572|26.00704|26.44216|
|Vegetation depth, ms|3.59780|3.06128|3.08928|3.49512|
|Vegetation shade, ms|3.81540|3.82824|3.82488|3.82232|
|CPU frame-call median, ms|1.75500|1.82000|1.67500|1.74000|
|Separate paced GPU p95, ms|27.03698|26.27299|26.23956|26.58431|
|Separate paced GPU p99, ms|28.07104|27.02063|27.19778|27.30196|
|All requested live bytes|405,332,792|405,332,792|405,332,792|405,332,792|
|All requested peak bytes|582,510,964|582,510,964|582,510,964|582,510,964|
|Observed DRM VRAM bytes|551,952,384|552,017,920|552,017,920|552,017,920|
|Observed DRM GTT bytes|20,099,072|20,279,296|20,279,296|20,180,992|
|Observed browser PSS bytes|845,895,680|849,556,480|850,068,480|844,514,304|

Complete frame spans include preparation, ordering, visibility, wood, coverage,
shading, updates, clouds, reconstruction and composition. All allocation includes
scratch, caches, original/fallback data, staging and outputs. This production scene
does not execute neural appearance decoding; no absent subsystem is credited as
free. CPU frame-call is not GPU completion or presentation. Physical DRM/PSS values
are deduplicated whole-client snapshots, not physical peaks or per-asset costs.
Do not sum UMA pools or interpret varying browser PSS as representation savings.

Startup support cooking was 1.86332 ms for the control and
0.99708 ms for the final variant in these recorded events.
These are isolated startup observations, not a separately established speed claim.
The support is rebuilt when the opt-in immutable atlas/Wood is recreated.

Because the final single-run saving is small, five alternating pairs test it:
control median **24.98448 ms**,
candidate **24.83992 ms**,
ratio-of-medians change **0.579% lower**.
The profiler reports **inconclusive**.
Paired changes (%): -0.4226, -0.6785, -0.6164, +0.0836, -0.6878.
CPU result: **inconclusive**.

The foliage depth pass improves consistently: 3.59612 → 3.45732 ms; all five pairs are faster (profiler: faster). That local gain is demonstrated, but it does not override the inconclusive complete-frame result. Requested allocation is unchanged; the observed DRM VRAM snapshot is 65,536 bytes higher. No representation-memory saving is claimed.

No repeated timing campaign was spent on the quality-failing approximate variants.

## Shaded quality and coverage

Each candidate batches three reference checks and four temporal scales in one
executable. Scoring follows the quiet GPU queue. Comparisons use the completed
16k/65k/independent-65k shaded references and the source-pinned retained baseline.
The candidate's own changed reference-check output is never substituted for truth.
Exact buffers are compared before reusing a metric result.

The following uses the fixed 65k reference. Ratios above one are worse.
Necessary static tolerances remain FLIP/LPIPS ≤1.05 and PSNR delta ≥−0.25 dB.
[Metric definitions and primary sources](citations.md#neural-reconstruction-instrument)
remain unchanged.

|Variant|Scale|FLIP ratio|LPIPS ratio|PSNR delta, dB|Tolerance|
|---|---:|---:|---:|---:|---|
|phased|1×|1.012862|1.161888|-0.526789|FAIL|
|phased|2×|1.005381|1.088366|-0.193511|FAIL|
|phased|3×|1.003504|1.048887|-0.029913|within|
|phased|4×|1.011739|1.014420|-0.069250|within|
|stable|1×|0.999012|1.109499|-0.204719|FAIL|
|stable|2×|1.000373|1.026273|+0.004987|within|
|stable|3×|1.003707|0.997477|-0.105925|within|
|stable|4×|0.997708|0.965079|+0.110190|within|
|constant|1×|1.000000|1.000000|+0.000000|within|
|constant|2×|1.000000|1.000000|+0.000000|within|
|constant|3×|1.000000|1.000000|+0.000000|within|
|constant|4×|1.000000|1.000000|+0.000000|within|

The approximate variants' native failures recur on all three fixed references.
Their results against the prior matched isotropic mip filter are also preserved
as quality comparators, not newly paired runtime claims. That filter was itself
rejected for 3× LPIPS; outperforming one of its metrics would not establish success.

The constant-region candidate exactly matches all four temporal scales and two
of three reference checks. The independent reference check differs in 24 color
components, by at most 0.0000152438879 linear RGB (mean displayed difference
1.078e-9). This small difference is recorded, not described as exact. All four
scored reconstruction images preserve their sampled silhouettes exactly. The full
1080p output and contrast-band differences are recorded below. The band is image-derived, not semantic silhouette
truth. These captures do not establish G3's broad camera/object-motion, reveal,
strand or trail gates. Existing references integrate production shading/filtering,
not independently converged light transport.

- phased: full 1080p maximum difference 137 byte levels; 97,874 pixels above two levels; contrast-band mean 1.342716 byte levels.
- stable: full 1080p maximum difference 137 byte levels; 102,829 pixels above two levels; contrast-band mean 1.404275 byte levels.
- constant: full 1080p maximum difference 1 byte level; 0 pixels above two levels; contrast-band mean 0.000001 byte levels.


The focused changed-certificate GPU guard evaluates 7×1024×512 = 3,670,016 queries.
It includes source atlases with eight, three and the production six leaves;
sparse/border coverage; uniformly soft coverage; fully opaque coverage; and
discontinuous coverage/leaf IDs. It spans all 16 patterns, six footprint widths,
support-cell boundaries, clamped UVs and partial leaf counts 1–8.
It compares the certified area against the original four actual reads and the
centre admission. An intentionally inflated certificate must produce detected
errors: 554,779 full and 1,319,207 empty certificates had zero false claims;
5,383 inflated-full and 74,467 inflated-empty errors were detected. Raw per-case counts and failed preparation runs remain in the evidence.
This is focused implementation validation, not broad scene qualification.

## Decision and next architecture bet

Retire both two-sample variants for native LPIPS failure. The constant-region variant preserves all four scored reconstruction images exactly and keeps requested allocation unchanged, but its five-pair timing comparison does not establish a complete-frame improvement. Restore parent and preserve all failures.

Test source-derived sparse pages for production foliage appearance: tiny exact descriptors for provably empty/constant pages, original samples for mixed boundary pages. First count page occupancy and all descriptor/boundary bytes to establish the attainable compression ceiling before implementing a decoder. Then compare complete decode/update/runtime and actual residency with a conventional occupancy-sparse control given the same source bounds. This differs from the rejected approximate source-coordinate chart and per-leaf response approximations; fewer reads alone cannot satisfy G4.
The two-sample failures show that matching a linear sampling average is insufficient
for this rendered pipeline. They do not prove that neural rendering or every
reduced-sampling method is impossible. No training or third optimization was tried.

All failed candidates, executable profiles, actual residency, captures, scores,
source snapshots, patches and helper scripts are preserved. Format/type checks
and the focused guard are the changed-code checks; unchanged machinery was not
repeatedly audited. All work ran solo on Pop!_OS with one AMD GPU queue.

Evidence: `/home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/leaf-phases-v1`. See replay.md and the archived scripts.

Manifest SHA-256: `5b158891b49e944c75a28683f27324b34824397729bc74c330c45f460db8a695`. Inherited benchmark edits remain unstaged.
