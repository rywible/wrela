# Exact sparse production appearance

Decision: **continue the fenced page representation as a conventional memory optimization
for the measured clearing. Change representation for the 16× G4 research path.** This is
a limited GPU memory result, not a neural win, frame-time speedup, total physical-memory
saving or G1–G7 pass.

## Comparison and useful result

Parent 2072092dbfb816672b27130d25b30364ff21e211; Wrela 0.9.0, compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. Solo on Pop!_OS, AMD RADV RENOIR
(1002:164c), Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57. No power/thermal lock.
One quiet GPU queue, captures batched into each executable, CPU scoring afterward.

The conventional control already has BC7 colour and two BC5 planes for normal and
toward/depth: 37,748,736 bytes over 12 production kinds. It receives the same source
fields, geometry, visibility and sampling information. We do not compare against an
inflated uncompressed atlas. Both sides retain identical geometry, sort/cull data,
24³ appearance volumes, near-leaf atlas and its support certificate.

The new opt-in path deduplicates the actual compressed block bytes on the GPU.
A shared map addresses three compressed palettes. All 9,437,184 source words are
compared against their packed destinations before activation. The original arrays
remain active during cooking and verification; successful activation releases them.
Normal source replacement rebuilds the pages. A small source-bounds readback delays
scratch allocation until the source bake is complete. This is the second and last
optimization followup in this block.

The allocated map and palettes total **15,728,640 bytes: 2.4× smaller** than the
37,748,736-byte BC impostors. Including unchanged volume, near-leaf and support
allocations makes the appearance reduction smaller. The whole renderer's requested
steady allocation drops by **22,020,096 bytes (5.43%)**.

| 1920×1080 clearing, internal scale 2× | Current conventional control | GPU pages, first attempt | Fenced pages, retained |
|---|---:|---:|---:|
| Complete saturated GPU median, ms | 24.95216 | 24.94336 | 25.00236 |
| Complete saturated GPU p95, ms | 26.55196 | 26.6148 | 26.73144 |
| Requested live bytes | 405,332,792 | 383,312,696 | 383,312,696 |
| Requested peak bytes | 582,510,964 | 631,486,896 | 578,009,512 |
| Observed DRM VRAM bytes | 551,952,384 | 535,240,704 | 535,302,144 |
| Observed DRM GTT bytes | 20,180,992 | 20,475,904 | 20,492,288 |
| Whole-browser PSS bytes | 841,558,016 | 873,338,880 | 797,790,208 |

Each profile has 600 warmup and 60 measured frames. It includes the complete renderer:
vegetation/wood, visibility, sorting, lighting, temporal reconstruction and composition.
All 660 frames and preparation timestamps are preserved. The same build is replayed
for requested-allocation accounting and actual process/driver snapshots, including
scratch, caches and fallback data.

Requested peaks are allocation high-water marks; physical figures are deduplicated
whole-client snapshots, **not physical peaks or per-asset attribution**. These are UMA
pools: do not sum VRAM, GTT and PSS. The PSS result reverses sign after a source edit,
so there is no established total physical-memory saving. Observed GPU residency is
lower in both measured scenes. Single-run timing differences do not establish speedup.

## Quality and source edits

Both candidates' four production HDR outputs (native, 2×, 3× and 4×) are byte-exact
against the retained conventional output at 192×108. Therefore all 12 comparisons
against the completed 16k/65k/independent-65k references have FLIP and LPIPS ratio 1.0,
PSNR delta 0 dB and unchanged image-derived contrast-band error. Metrics are reused
only after exact-buffer verification. The candidate's own reference-check captures
are not replacement truth: they differ in 5/13/8 components, maximum linear errors
1.90735e-6 / 4.76837e-7 / 1.43051e-6.

The separate 1080p fenced image is not exact: maximum error 9 byte levels, five pixels
above 2 levels, mean RGB error 0.00013825 levels, alpha exact. Its same-build replay
differs by at most one level in ten components. Exact BC payload does not guarantee
identical floating-point filtering. These differences are preserved. The contrast
band is image-derived, not independent semantic silhouette truth. This one fixed
shot does not qualify unseen views, motion, G3 or independent light transport.

For source-update qualification, both methods rebuild all 12 kinds at frame 300
using seeds k+101 instead of k+1, through the real remake_woods and caster/occluder
invalidation paths. The changes are measurement hooks and are restored afterward.
All subsequent packing/verification and old/new residency are included.

| After the same source change | Conventional control | Fenced pages |
|---|---:|---:|
| Complete steady GPU median, ms | 26.06396 | 25.99296 |
| Whole-frame GPU sum, edit frames 300–359, ms | 2,310.27607 | 2,334.76789 |
| Largest whole-frame GPU span in window, ms | 152.89981 | 153.39529 |
| CPU frame-call sum in window, ms | 263.25000 | 253.83500 |
| Elapsed window, ms | 2,499.07003 | 2,526.66521 |
| Largest frame-start interval in window, ms | 228.08984 | 233.76001 |
| Requested live bytes | 433,085,868 | 411,065,772 |
| Requested peak bytes | 633,016,412 | 610,996,316 |
| Observed DRM VRAM bytes | 585,494,528 | 577,044,480 |
| Observed DRM GTT bytes | 20,525,056 | 20,901,888 |
| Whole-browser PSS bytes | 857,817,088 | 869,979,136 |

This is about 24.49 ms additional GPU work across the matched 60-frame edit window,
with a 27.60 ms longer elapsed window in this pair. It is not a universal update bound.
The whole-frame spans include work outside page cooking; GPU work and overlapping
CPU time must not be added. Label scopes can extend to subsequent work, so individual
preparation labels are not treated as isolated kernel costs. Initial 60-frame GPU
sums were 8,575.49 versus 8,639.82 ms; elapsed 10,393.49 versus 10,466.43 ms. Startup
includes compilation and source preparation and varies between runs.

The changed-source final 1080p image differs by at most eight byte levels; eight
pixels exceed two levels, mean RGB error 0.00014226 levels, alpha exact. The ordinary
repeat is exact; the packed repeat differs by one level in two components. This
qualifies this source replacement, not broad temporal quality. The second source
has 198,362 unique blocks; both source payloads pass every-word verification.

## Why this cannot reach G4

The initial executable exported all 12 production compressed atlases and both
volume atlases in one run. CPU roundtrips verified every exact dictionary estimate.
The finest joint 4×4 block census found 199,702 unique 48-byte triples among 786,432
blocks: 9,585,696 payload bytes plus 3,145,728 map bytes, ideally 2.965× smaller.
The power-of-two GPU palette rounds that ideal up to the measured 2.4×.

Even the descriptor-free exact-block payload exceeds the original BC storage
divided by 16. This rejects 16× for this exact page family on this source corpus;
it is not a universal entropy bound or a claim about all representations. Larger
pages were worse. Joint pages were better than independent plane dictionaries.
Volume 2³-voxel pages had an exact 568,464-byte estimate (4.669×), but no volume
renderer was implemented. View cropping is likewise a diagnostic estimate, not a
rendered win. These unimplemented possibilities are not added to the result.

The initial census finished in 11.71 minutes, the first complete GPU-quality-memory
assessment in 32.89 minutes, and the decision in 52.92 minutes.
Exactly two optimization followups were used: GPU pages, then deferred allocation.
No training or reference-algorithm changes occurred. The first unfenced attempt's
higher peak and PSS, all compiler failures and every capture are preserved.

## Verification and next bet

The actual GPU verifier passed on both source populations. A negative control XORed
every packed word by 1 without changing the source; the real verification path
detected corruption and prevented activation. It is restored to measured source.
Formatting and focused clearing type/effect checks pass. The file allowlist initially
rejected 27 accumulated reports from this research branch; explicit entries for those
reports and this one are added, without broadening the allowlist pattern.

Continue this as a measured, opt-in conventional GPU memory tradeoff; ordinary
compressed/uncompressed paths remain available. It is enabled for the clearing only.
Hash overflow or failed readback/verification fails closed; graceful fallback on
these failures, other assets/devices and broader animated scenes are not qualified.

For the larger G4 bet, share a source-space appearance factorization across angular
views, with explicit coverage/depth residuals that preserve visibility. Compare an
ordinary low-rank or analytic decoder and any small learned decoder using identical
field information. Establish the rendered memory/runtime ceiling before fitting.
Avoid re-running the retired moment-only, full-field and material-error loops without
a new rendered hypothesis.

Evidence: build/neural-evidence/sparse-appearance-v1 on the Pop!_OS worktree.
Its plan, census, two complete assessments, source-update assessment, corruption
control, archived sources/builds, raw captures and replay scripts are sealed together.
Fixed references and the retained baseline are linked by dependency manifests.
Manifest SHA-256: `6b989c2026312f6e1ac745dca2038a58dd3a65a440c37328c189ec435ce41378`.
