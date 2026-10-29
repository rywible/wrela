# Streamed appearance construction: retain the bounded scratch

2026-10-29. **Decision: continue this construction architecture for both conventional
BC pages and source IDs.** It reduces requested and observed GPU resident peaks
during startup and source edits, preserves appearance data and the measured
shaded/silhouette quality, and keeps complete frame time comparable. Steady
representation bytes are unchanged. Browser process memory is mixed; this is
not a total physical-memory or frame-rate win. G1–G7 remain unchanged and unmet.

The first joint assessment completed in **18.37 minutes**
from registration, under 28 minutes from the preceding checkpoint commit.
One targeted follow-up reused the source-ID raster; the second slot was unused.
Decision at 43.47 minutes from registration.
Solo on Pop!_OS AMD, no training and no new reference targets or algorithms.

## Question and equal controls

The prior renderer kept every kind's full colour/normal bake until constructing
the wood's pooled storage. Wrela already knows each authored plant's card and
limb counts. PoolCook reserves that storage once and installs each deterministic
source immediately, using one 16 MiB colour/normal/depth scratch set.

The same change applies to exact deduplicated BC pages and source-ID palettes.
Both receive identical plants, source code, cards, limbs, visibility, sampling,
lighting, ordering and updates. Existing standalone kind/bake/Wood::new APIs
remain usable; the clearing opts into incremental construction. Its two-kinds-
per-frame planner is preserved. Page verification and original BC fallback
remain; this is not direct encoding into final compact storage.

Initial streaming exposed a second peak in the ID path: twelve separate
source-ID/depth rasters. The single follow-up shares one 8 MiB raster pair
across ordered raster, encode and verify commands. The ID format, decoder,
palette, bounds and acceptance logic are unchanged.

## Whole-renderer requested allocation

Bytes, including scratch, caches, metadata, fallback and common resources.
Live bytes are identical before/after within each row; they include the entire
renderer, not only appearance.

| Method | Steady live bytes | Prior peak | Streamed peak | Peak reduction |
| --- | ---: | ---: | ---: | ---: |
| Exact BC pages | 383,312,696 | 578,009,488 | 475,022,248 | 17.82% |
| Source IDs | 373,069,104 | 578,009,512 | 460,157,068 | 20.39% |
| BC after source edit | 411,065,772 | 610,996,316 | 499,604,916 | 18.23% |
| IDs after source edit | 400,771,796 | 600,752,724 | 479,485,372 | 20.19% |

Initial streaming without the follow-up gave the ID path a 552,431,756-byte
peak; the final 460,157,068-byte result replaces it. Both are preserved.
The old BC replay recorded 578,009,488 bytes versus the prior report's
578,009,512; the 24-byte variation is retained, not normalized away.
Timed observer replays add timestamp resources (4,096 steady bytes);
their raw counts remain in the evidence.

Final impostor representation storage stays **15,728,640 bytes for BC** and
**5,485,048 bytes for source IDs**. Geometry, volume atlases and other common
appearance remain resident. Neither this ratio nor the peak reduction is
the 16× complete appearance-memory result required by [G4](neural-rendering-goals.md).

## Actual resident observations

Fresh memory replays sample the Chrome process group through startup and
660 frames. The edit runs also rebuild all twelve source individuals at
frame 300 using seed offset 101. DRM clients are deduplicated; PSS is summed
across the browser process group. Decimal MB below.

| Run | Sampled maximum DRM VRAM, prior → streamed | GTT | Browser PSS |
| --- | ---: | ---: | ---: |
| BC startup | 724.718 → 617.624 | 20.476 → 20.722 | 4508.623 → 4601.869 |
| ID startup | 724.709 → 607.314 | 20.476 → 20.722 | 4607.895 → 4494.748 |
| BC edit run | 837.501 → 660.812 | 21.000 → 21.131 | 4472.692 → 4473.508 |
| ID edit run | 822.088 → 637.063 | 20.902 → 21.311 | 4524.748 → 4604.012 |

The sampler sleeps 50 ms plus process-read time; sample counts and maximum gaps
are recorded. These are observed maxima, not guaranteed continuous physical
peaks. Do not add UMA VRAM, GTT and PSS: their accounting can overlap.
No simultaneous CPU scoring or builds ran during timing.

Startup browser PSS reaches roughly 4.5–4.6 GB in both architectures. This
was missed by previous end-only snapshots; its cause is not established here.
Across the four fresh normal-scene pairs, end-of-run VRAM changes range
-13.029 to -12.653 MB and PSS changes
-22.432 to 15.707 MB.
BC endpoint PSS rises by 5.922–15.707 MB; ID endpoint PSS ranges from
−22.432 to +9.253 MB. After source edits, steady VRAM is nearly unchanged;
BC PSS rises by 7.976–15.709 MB and ID PSS falls by 2.674–5.485 MB.
Lower requested and GPU-resident peaks do not establish lower total physical
memory. All endpoint and sampled observations, including unfavorable ones,
remain in the evidence.

## Complete rendering and update cost

AMD RADV RENOIR, Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 / compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
1920×1080 output with the normal internal 2× reconstruction, one worker,
application adaptive measurement/pacing disabled. No power or thermal lock;
conditions are recorded before and after each repeat.

Already-built binaries ran in fresh browsers, in forward then reverse order.
Each has 600 warmup plus 60 steady measured frames. Full GPU span is the
earliest pass start to latest pass end for each frame: decoding, sorting,
visibility, wood, lighting, compositing and the rest of the renderer all count.
CPU frame call and browser intervals are reported separately. These quiet
timing replays use the allocation observer but omit continuous residency
sampling; primary profiles and sampled-residency runs are preserved separately.

| Fresh run | Full GPU p50 ms | p95 | p99 | CPU p99 ms | Browser interval p99 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| repeat1-parent-bc | 25.034 | 26.586 | 27.523 | 2.350 | 29.579 |
| repeat1-stream-bc | 25.035 | 26.773 | 27.558 | 2.083 | 28.992 |
| repeat1-parent-id | 25.016 | 26.706 | 27.640 | 1.915 | 30.201 |
| repeat1-shared-id | 25.036 | 26.725 | 27.761 | 2.106 | 30.344 |
| repeat2-shared-id | 25.050 | 26.760 | 27.514 | 2.200 | 29.070 |
| repeat2-parent-id | 25.047 | 26.774 | 27.583 | 1.981 | 29.769 |
| repeat2-stream-bc | 25.090 | 26.666 | 27.374 | 2.112 | 29.685 |
| repeat2-parent-bc | 25.033 | 26.944 | 27.712 | 2.137 | 29.197 |

Paired steady median changes span +0.003%
to +0.225%. This supports comparable
runtime in this scene, not a speedup claim.

Startup work is also charged. GPU sums cover frames 0–119; browser elapsed
covers starts of frames 0→120, exactly 120 intervals. The latter does not include
browser launch or work before its first frame, and is not advertised as total
cold-load wall time.

| Fresh run | First 120 complete GPU spans, sum ms | First 120 browser intervals, ms |
| --- | ---: | ---: |
| repeat1-parent-bc | 10238.658 | 12081.150 |
| repeat1-stream-bc | 10347.484 | 12247.530 |
| repeat1-parent-id | 10553.085 | 12602.760 |
| repeat1-shared-id | 10377.962 | 12267.625 |
| repeat2-shared-id | 10441.266 | 12357.360 |
| repeat2-parent-id | 10343.708 | 12245.760 |
| repeat2-stream-bc | 10406.239 | 12336.535 |
| repeat2-parent-bc | 10261.743 | 12131.200 |

For a real source replacement, the following 60-frame window is frames 299–358;
elapsed is start 299→359, exactly 60 intervals. Both representations rebuild,
verify and activate successfully. These are separate fresh forward/reverse
runs of the corresponding changed-source binaries, not comparisons against an
unchanged scene.

| Fresh edit run | Full GPU sum ms | Browser elapsed ms | Worst GPU frame ms | Worst CPU call ms |
| --- | ---: | ---: | ---: | ---: |
| repeat1-parent-update-bc | 2335.350 | 2530.540 | 153.115 | 148.045 |
| repeat1-update-stream-bc | 2334.408 | 2521.540 | 151.244 | 146.095 |
| repeat1-parent-update-id | 2350.458 | 2554.205 | 155.138 | 148.070 |
| repeat1-update-shared-id | 2350.290 | 2537.845 | 151.518 | 145.610 |
| repeat2-update-shared-id | 2343.805 | 2532.205 | 150.992 | 144.200 |
| repeat2-parent-update-id | 2349.370 | 2554.185 | 153.925 | 147.125 |
| repeat2-update-stream-bc | 2333.141 | 2518.870 | 151.243 | 144.345 |
| repeat2-parent-update-bc | 2338.500 | 2534.710 | 153.531 | 147.280 |

BC's first 120 frames cost about 109–145 ms more summed GPU time and
166–205 ms more browser elapsed in these two pairs. ID startup differences
change sign. Edit-window differences are small. The decision accepts this modest
BC startup cost for the measured peak-memory reduction; it does not erase it
behind steady-state medians.

## Rendered quality and silhouette evidence

- All **38 pooled outputs** match the completed control byte for byte:
  twelve BC colour, normal and depth images plus the two full volume atlases.
  They are exported together from one initialized executable.
- All **12,582,912 final decoded source IDs** match the completed original raster
  exports. Twelve outputs share one initialized executable, and export only after
  all kinds have been packed. This checks scratch lifetime across kinds, beyond
  the existing per-kind early verifier.
- All **24 established shaded stress checks** pass against the same three
  completed quadrature references. This forced-impostor shot includes normally
  card-rendered crowns; it is not normal production LOD.
- All **12 fresh 1080p shaded checks** pass the existing 5% FLIP/LPIPS and
  −0.25 dB PSNR limits. The completed 256/1024/independent-1024 references are
  reused; candidate captures never become their own truth.
- Static native leaf-class coverage has precision 99.999303%,
  recall 100.000000%, symmetric boundary p99
  0.000 px and maximum 2.828427 px.
  The class includes impostor wood but excludes separately drawn near wood.
  This is not strand/branch/instance truth, broad motion or a G3 pass.

All four final native temporal HDR captures are byte-identical to the retained
source-ID captures. The integrated diagnostic coverage alpha is also exact;
its RGB differs in 45 float values, maximum absolute 0.000159145. That diagnostic
is not a new reference target or evidence of improved physical accuracy.

Direct comparisons with the retained ID captures, native image differences,
and every profile/edit/repeat screen delta are recorded in final-assessment.json.
Full screens are not claimed byte-identical merely because the baked data are.
The frozen source-ID wood approximation remains; the rejected exact-wood variant
is not reintroduced.

## Verification, failures and next bet

The established wood-merge GPU tool test exercises the legacy Wood::new path
with uncompressed storage. The clearing typechecks; formatting and workspace
allow checks pass. No unchanged shader/verifier/capacity suite was repeatedly
audited. One failed ownership/Texture.clone build is archived with its source
and diagnostics; the repaired ownership encodes scratch directly.

Continue bounded construction because the measured quality and runtime remain
acceptable while both controls use materially less peak GPU memory. For further
G4 progress, **change the representation/lifetime**, rather than tune frozen
material moments: generate only the currently needed view tiles from Wrela's
deterministic source, under a fixed cache budget. Give both BC and source-ID
controls the same demand and visibility information. Charge misses, edits,
silhouette uncertainty and fallback before keeping it. This is a next bet,
not an implemented or demonstrated result.

Evidence lives on Pop!_OS at
/home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/streamed-bake-v1.
The sealed bundle contains source archives, executable builds, exact commands,
all captures, profiles, allocation/DRM/PSS observations, scripts, frozen hashes,
failed attempts and replay instructions. Parent commit:
7f4e756c5f33aaf6a0999a82653d6b38731a4d9f.
Manifest SHA-256: 54960ca5a485e201b882b947248e40b287edf64a9f30288c9ca2de8a9a6f44aa.

Prior evidence and limitations: [exact BC pages](neural-sparse-appearance.md),
[source IDs](neural-source-identity-atlas.md), and
[retired wood records](neural-source-wood-records.md). No new external algorithm
is introduced; the established metric citations remain in [citations](citations.md).
