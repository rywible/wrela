# Sparse wood records: retire the extension, keep source IDs

2026-10-29. **Decision: retire this wood-record variant.** Restore the retained
[source-ID runtime](neural-source-identity-atlas.md) from d707a30a. The experiment
made wood attributes match the original bake exactly, but the rendered result
does not justify its extra storage and resident memory. There is no new rendering
win in this block. G1–G7 remain unchanged and unmet.

The first joint assessment took **9.29 minutes from plan registration**, less
than 13 minutes even measured from the preceding checkpoint commit. One targeted
optimization followed; the allowed second slot was unused. Decision at
32.25 minutes from registration.
Solo on Pop!_OS AMD. Frozen models, zero training cycles. No new reference
algorithm or reference targets.

## Question and fair controls

The previous source-ID path reconstructs wood attributes from triangle
barycentrics. Against the pre-BC bake, 322 of 86,919 visible wood texels differed
by more than one degree, maximum about 14.10 degrees. That is a discrepancy
against the raster bake, not proof of error against independent physical truth.

The candidate stores two half2 words per visible wood texel: octahedral normal,
toward coordinate and depth. A 16-bit mask and local record pointer attach to
each affected 4×4 block. Cards retain their existing source-ID reconstruction.
All 12,582,912 source IDs and every stored wood attribute must verify before
activation. Original BC fallback data remain resident during construction.

Both controls receive the same source cards, limbs, fields, visibility,
sampling, lighting, ordering, updates and compositing. Compare against both
deduplicated BC pages and the already-retained source-ID path. A ratio against
BC alone cannot establish improvement over the smaller source-ID control.

## Representation and full cost

| Representation | Impostor bytes | Change versus retained IDs |
| --- | ---: | ---: |
| Exact BC pages | 15,728,640 | +186.76% |
| Retained source IDs | 5,485,048 | — |
| Sparse wood records | 6,366,968 | +881,920 B / +16.08% |

Of the addition, 695,352 bytes are the 86,919 eight-byte wood records; the rest
is routing overhead. Common appearance resources and geometry remain identical
and resident. These numbers are not a complete G4 appearance-budget ratio.

The first implementation created separate bake scratch for every kind:
requested whole-renderer startup peak **578,009,512 → 785,526,924 bytes**.
The single follow-up writes IDs and attributes in one depth pass and reuses
one 16 MiB raster scratch set across kinds. It restores the peak to
**578,009,512 bytes**. All four stress HDR captures are byte-identical before
and after that optimization. Repairing this candidate's own regression is
not an improvement over either retained control.

Requested steady GPU allocation is 383,312,696 bytes for BC,
373,069,104 for source IDs and 373,951,024 for wood records. These observations
count scratch, metadata, fallback data, caches and the complete renderer;
startup peak includes temporary resources. No common resources are excluded
from the whole-renderer observations.

### Fresh paired runtime and actual residency

Already-compiled production binaries ran in fresh Chrome sessions in order
BC, IDs, wood, wood, IDs, BC. Each run uses 600 warmup plus 60 measured frames,
1920×1080 output, internal 2× reconstruction, one worker, pacing/adaptive
measurement disabled in the application. Scoring/builds did not run concurrently.

The complete GPU span is maximum timestamp end minus minimum start per frame.
It includes decoding, sorting, visibility, wood, current updates, lighting and
compositing. Ranges below span two runs; they are not confidence intervals.

| Method | Full GPU p50, ms | p95, ms | p99, ms | Observed DRM VRAM, MB | Browser PSS, MB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Exact BC pages | 24.946–25.035 | 26.466–26.774 | 27.575–27.578 | 535.249–535.249 | 824.296–826.079 |
| Retained source IDs | 25.027–25.131 | 26.601–26.640 | 27.594–27.606 | 519.786–519.844 | 834.574–834.674 |
| Wood records, shared scratch | 25.002–25.086 | 26.649–26.864 | 27.521–27.585 | 520.626–520.692 | 835.984–837.605 |

There is no repeatable full-frame runtime improvement. Compared with source
IDs, wood records add about 0.84–0.85 MB observed VRAM and 1.31–3.03 MB paired
browser PSS. The raw timing replays include their small timestamp allocations;
the allocation-only totals above omit that instrumentation. GTT, per-process
readings, CPU times, browser intervals and all individual samples are retained.
Do not sum overlapping UMA memory pools. These are residency snapshots, not
physical-memory peaks, and no total physical-memory reduction is claimed.

Hardware/software: Ryzen 5 5500U, AMD RADV RENOIR PCI1002:164c, Mesa26.1.6,
Chrome154.0.8037.57, Wrela0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. No power or thermal lock.
Each fresh replay records available temperature/governor/frequency observations.

## Shaded quality, silhouettes and attribute diagnostics

Both candidate layouts pass all 24 established forced-impostor stress checks.
The final candidate passes all 12 production 1080p checks against completed
coarse/fine/independent quadrature references, using unchanged gates:
FLIP/LPIPS ≤1.05×, PSNR loss ≤0.25 dB. The stress forces some normally-card crowns
to impostors; the native check uses production detail selection.

Relative to retained source IDs across those checks:

| Metric | Best change | Worst change |
| --- | ---: | ---: |
| FLIP error | −0.014360% | +0.002554% |
| LPIPS error | −0.133792% | +0.076151% |
| PSNR | +0.000773 dB | −0.000354 dB |

These tiny mixed changes do not establish a rendered-quality advantage or meet
the charter's quality-promotion criterion. No observer study or clip-level
confidence claim follows from these static development samples.

Leaf-class coverage metrics are unchanged from retained IDs: four additional
thresholded pixels, none missing; precision 99.999303%, recall 100%, boundary
p99 zero and maximum 2.82843 pixels. This includes wood inside impostors but not
separately drawn near wood. It is not instrumented strand/branch truth or G3
motion qualification. Reference convergence and other limits are inherited
from the [completed comparison](neural-source-identity-atlas.md).

The independent pre-BC attribute probe covers all 12 kinds and finds zero
coverage mismatches. All 86,919 wood normals and toward/depth values match
exactly; zero wood texels exceed one degree. Card diagnostics are unchanged.
This is a successful diagnostic fix whose rendered benefit does not pay for
the added data on this measured workload.

## Actual source replacement and failure controls

Both the retained controls and candidate rebuild all 12 kinds at frame 300 with
seeds k+101 rather than k+1, through the real source replacement and caster/
occluder invalidation path. Control captures and profiles are the completed
unchanged-source-edit measurements; the candidate is measured here. They are
not a fresh alternated edit-timing study, so small differences are not speed claims.

| Method | 60-frame GPU sum, ms | Elapsed, ms | Requested live after edit, B | Requested edit peak, B |
| --- | ---: | ---: | ---: | ---: |
| exact-bc | 2331.58783 | 2551.04004 | 411,065,772 | 610,996,316 |
| source-id | 2344.15823 | 2565.47998 | 400,771,796 | 600,752,724 |
| wood-shared | 2350.30891 | 2573.27002 | 401,640,692 | 601,634,644 |

GPU spans use frames 299–358 inclusive. Elapsed uses start-of-frame 299 to
start-of-frame 359, all 60 intervals; the earlier report stopped at start 358.
Every rebuilt source ID and wood record verifies. The candidate has no update
advantage: it adds approximately 0.88 MB to requested edit peak versus IDs.
Final changed-source image diagnostics are retained; no newly converged
changed-population reference or broad motion claim is made.

Flipping a stored wood-normal bit makes the actual verifier reject activation.
Constraining payload capacity to 16 words beyond the map selects the exact BC
page path, whose existing full-word verifier passes. Both controls and the source
edit restore their hooks exactly. These checks validate the experiment, not a
rendering win.

## Next architecture bet and preserved evidence

Stop spending storage on this discrepancy until a rendered failure makes it
consequential. Next test **streaming each kind's source bake directly into its
final representation**, retiring temporary textures as soon as that kind is
installed. Apply the same construction/residency policy to BC pages and source
IDs. The target is the measured startup/edit peak, not another material-moment
score; complete cost and failure/fallback behavior still count.

All runtime changes from this block are restored. Both candidate source versions,
failed build attempts, compiled binaries, raw HDR/coverage captures, all-kind
probes, timings, resident-memory readings and replay scripts remain under
build/neural-evidence/source-wood-records-v1 on Pop!_OS. The initial high-memory
failure is preserved alongside the shared-scratch follow-up.

References: [source-ID control and reference limits](neural-source-identity-atlas.md),
[exact BC control](neural-sparse-appearance.md), [unchanged goals](neural-rendering-goals.md),
[measurement rules](bench.md), [metric sources](citations.md#neural-reconstruction-instrument).
Own implementation, no external model, code or asset imports.

Evidence manifest SHA-256: 64e5df8f8a9495a749090ec90d902f162d07740d157479cace6190d0589ac0ee.
