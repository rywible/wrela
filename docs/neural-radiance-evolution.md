# Declared lighting factors improve conventional temporal reconstruction

2026-10-29 UTC. Parent `6d701aebb40df378b286f74c2f37b331a68746cc`.

**Decision: continue sixteen-frame source-declared radiance rebasing as an
opt-in conventional control. Retire both eight-frame alternatives.**
Across six public clips at4×, raw-factor mean FLIP falls **0.154857→0.116796**
(24.58% lower), LPIPS falls 9.70%, PSNR rises
1.24dB, and opaque temporal residual falls 24.38%.
Requested live storage is unchanged at92,013,084B. Complete initial AMD medians are
12.54320ms ordinary versus12.55448ms rebased. The final public-selector build is
12.51164ms; paired costs below bound this result more usefully than that unpaired difference.

First joint assessment: **12.46minutes**.
Architecture decision after exactly two followups: **22.60minutes**.
No new training, changed goal, new reference algorithm, production rendering win,
G1–G7 qualification or world-leading claim.

## What source knowledge contributes

The existing nonlinear leaf diagnostic declares a global illumination multiplier
from1 to0.35 at tick16 for cases76/77. Its sun direction continues to rotate.
The same source declaration now supplies both shading and a three-value event
description: change tick, prior multiplier and current multiplier. No new source
RGB query, target pixel, future observation or fitted parameter enters decoding.

Cheap source-aware history multiplies its previous linear-RGB observation by the
current/previous factor before the existing view-space blend. Raw-factor decoding
multiplies each retained observation by its current/capture factor before weighting
or nearest-compatible fallback. Current observations, geometry rules, sixteen
history phases, query tables and compositing remain unchanged.

The controls receive the same useful source information: ordinary history,
factor-rebased history, and explicit reset at the declared change are compared
for both cheap and raw-factor methods. The unrestricted direct-source method
receives full authored appearance and its queries are timed. Its additional
access is disclosed; it does not satisfy G1's sixteen-times-fewer-colour-samples
requirement merely by producing low error.

This is a conventional control, not a claim of novel temporal reconstruction.
[AMD's FSR2 manual](https://gpuopen.com/manuals/fidelityfx_sdk/techniques/super-resolution-temporal/)
already documents exposure/pre-exposure inputs and shading-change handling.
Here the tested factor is a declared physical irradiance change. No AMD code is
copied, and no FSR performance or tournament result is inferred.

On AMD,73,728 evaluations of the existing material response at fixed geometry,
direction and material agree with scalar factorization to a maximum linear-RGB
difference of **1.7881393e-7**.24,576 evaluations use the changed factor.
A deliberately1%wrong predicted factor fails the guard. The initial test tried
calling the checked case catalog inside GPU code; its possible panic was rejected
before execution. That failed source/log is preserved; the repaired guard binds
the four source fields from the CPU. This does not prove arbitrary relighting,
zero-intensity recovery, directional edits, visibility changes or material edits.

## Complete cost and actual residency

Same Pop!_OS AMD RADV RENOIR PCI1002:164c / Ryzen5 5500U, Mesa26.1.6,
Chrome154.0.8037.57, Wrela0.9.0/compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
Timing is case77,1920×1080,4×,one worker,120frames with **zero warmup** so
the cold frame and declared event16 are included. This differs from earlier
60-warmup fixtures and their timings must not be mixed into a speedup claim.
All GPU work is sequential; metric scoring follows it.

| Method | Saturated complete median / p95 ms | Paced median / p95 / p99 ms | Paced event16 ms |
|---|---|---|---|
| cheap | 2.09076 / 3.73873 | 2.65080 / 3.17562 / 6.04737 | 2.68632 |
| raw | 12.54320 / 13.68760 | 12.70840 / 13.85589 / 16.13145 | 12.54908 |
| direct | 3.57960 / 4.94047 | 3.59264 / 5.10269 / 13.22868 | 3.78556 |
| cheap-rebase | 2.02960 / 3.34588 | 2.67168 / 2.97663 / 5.47173 | 2.54584 |
| raw-rebase | 12.55448 / 13.62516 | 12.71680 / 13.95526 / 15.56724 | 12.59304 |
| cheap-reset | 2.05928 / 3.00432 | 2.64356 / 3.32127 / 6.47029 | 2.41580 |
| raw-reset | 12.53496 / 13.72056 | 12.73866 / 14.05617 / 16.36850 | 8.45112 |
| raw-rebase-8 | 11.55872 / 12.67224 | 11.61886 / 12.70099 / 15.38166 | 10.98764 |
| raw-rebase-8fixed | 10.79756 / 11.71344 | 10.91670 / 12.02051 / 15.99921 | 9.97612 |
| cheap-rebase-final | 2.01056 / 3.15660 | 2.54428 / 3.09148 / 6.26102 | 2.16148 |
| raw-rebase-final | 12.51164 / 13.58856 | 12.56804 / 14.04797 / 15.53437 | 12.46236 |

Saturated and paced distributions are recorded separately. Cold/startup tails
are included, not a steady-state presentation guarantee. All observation
generation, packing, query tables, history updates, decoding and display work
count. These remain complete analytic-fixture frames, **not** complete production
clearing frames with wood,3D visibility/sorting and production compositing.

| Method | Requested live / peak B | Actual resident VRAM / GTT B | Browser PSS B |
|---|---|---|---|
| cheap | 48,021,584 / 56,315,984 | 95,125,504 / 18,427,904 | 550,740,992 |
| raw | 92,013,084 / 100,307,484 | 145,031,168 / 18,427,904 | 594,665,472 |
| direct | 25,212,040 / 33,506,440 | 69,849,088 / 10,133,504 | 595,420,160 |
| cheap-rebase | 48,021,584 / 56,315,984 | 95,121,408 / 18,427,904 | 549,920,768 |
| raw-rebase | 92,013,084 / 100,307,484 | 145,031,168 / 18,427,904 | 560,920,576 |
| cheap-reset | 48,021,584 / 56,315,984 | 95,125,504 / 18,427,904 | 550,364,160 |
| raw-reset | 92,013,084 / 100,307,484 | 145,031,168 / 18,427,904 | 550,774,784 |
| raw-rebase-8 | 71,277,084 / 79,571,484 | 124,293,120 / 18,427,904 | 592,849,920 |
| raw-rebase-8fixed | 71,277,084 / 79,571,484 | 124,227,584 / 18,427,904 | 591,794,176 |
| cheap-rebase-final | 48,021,584 / 56,315,984 | 95,125,504 / 10,133,504 | 547,734,528 |
| raw-rebase-final | 92,013,084 / 100,307,484 | 145,031,168 / 18,427,904 | 592,983,040 |

Each memory measurement is a separate300-frame replay of the same compiled build.
Requested accounting covers all fixture resources, history, tables, scratch,
output and unused common allocations. DRM snapshots are deduplicated by
device/client; PSS covers the browser process group. These are whole-client
resident observations, not per-asset physical peaks; do not sum UMA pools.

Three alternating pairs per retained method use the same cold120-frame protocol
against its ordinary16-frame control. These small descriptive comparisons do not
establish a sustained speedup or general hardware confidence interval.

- cheap: paired GPU changes -0.195%, +0.277%, -0.885%; tool status **inconclusive**. Baseline/candidate median across pairs 2.01064/2.00552ms.

- raw: paired GPU changes -0.137%, +0.378%, -0.064%; tool status **inconclusive**. Baseline/candidate median across pairs 12.50088/12.52332ms.

## Rendered quality and the failed memory reductions

Cases1,23,74,75,76,77;192×108;32coldframes;scales1–4. Existing shaded targets
and visibility masks are generated after reconstruction and never bound as
decoder inputs. All comparisons require matched targets and metric conditions:
Reinhard exposure1,FLIP67pixels/degree,LPIPS AlexNet0.1 and sRGB PSNR.
Below are equal-case means. Per-clip/native-scale values and transition frames
remain in evidence; boundary RGB diagnostics do not certify geometric silhouettes.

| Method | Scale | FLIP | LPIPS | PSNR dB | Temporal residual | Mixed-boundary RGB MAE |
|---|---|---|---|---|---|---|
| cheap | 2 | 0.190286 | 0.296661 | 21.106223 | 0.009816 | 0.072417 |
| cheap | 3 | 0.218968 | 0.402173 | 19.687990 | 0.011013 | 0.077193 |
| cheap | 4 | 0.251451 | 0.460331 | 18.683159 | 0.011549 | 0.084556 |
| raw | 2 | 0.112966 | 0.066313 | 25.811906 | 0.008303 | 0.023410 |
| raw | 3 | 0.134277 | 0.175940 | 23.043387 | 0.009993 | 0.035219 |
| raw | 4 | 0.154857 | 0.186073 | 21.961209 | 0.011068 | 0.039159 |
| direct | 2 | 0.002261 | 0.000005 | 75.858187 | 0.000026 | 0.000113 |
| direct | 3 | 0.002261 | 0.000005 | 75.858187 | 0.000026 | 0.000113 |
| direct | 4 | 0.002261 | 0.000005 | 75.858187 | 0.000026 | 0.000113 |
| cheap-rebase | 2 | 0.178110 | 0.296219 | 21.641360 | 0.008392 | 0.069934 |
| cheap-rebase | 3 | 0.207127 | 0.402518 | 20.167600 | 0.009597 | 0.074705 |
| cheap-rebase | 4 | 0.240314 | 0.460591 | 19.096829 | 0.010193 | 0.081968 |
| raw-rebase | 2 | 0.068729 | 0.055917 | 28.223145 | 0.005444 | 0.014388 |
| raw-rebase | 3 | 0.092993 | 0.152157 | 24.604589 | 0.007272 | 0.027051 |
| raw-rebase | 4 | 0.116796 | 0.168024 | 23.203731 | 0.008370 | 0.031399 |
| cheap-reset | 2 | 0.179760 | 0.302858 | 21.645265 | 0.009100 | 0.069837 |
| cheap-reset | 3 | 0.208280 | 0.408531 | 20.170308 | 0.010314 | 0.074539 |
| cheap-reset | 4 | 0.242302 | 0.466409 | 19.080199 | 0.010940 | 0.082125 |
| raw-reset | 2 | 0.072707 | 0.074795 | 27.793194 | 0.006632 | 0.014983 |
| raw-reset | 3 | 0.096474 | 0.164658 | 24.349616 | 0.008574 | 0.027643 |
| raw-reset | 4 | 0.121029 | 0.173981 | 23.003758 | 0.009714 | 0.032017 |
| raw-rebase-8 | 2 | 0.076499 | 0.079738 | 26.889109 | 0.010501 | 0.017375 |
| raw-rebase-8 | 3 | 0.106929 | 0.181104 | 23.504863 | 0.013639 | 0.030302 |
| raw-rebase-8 | 4 | 0.134245 | 0.205344 | 22.185745 | 0.016086 | 0.035139 |
| raw-rebase-8fixed | 2 | 0.076342 | 0.082441 | 26.803260 | 0.010547 | 0.017421 |
| raw-rebase-8fixed | 3 | 0.105624 | 0.188172 | 22.927257 | 0.014729 | 0.031045 |
| raw-rebase-8fixed | 4 | 0.127863 | 0.212367 | 21.612288 | 0.018060 | 0.036054 |

At4× case77, event-frame FLIP is0.508579 for raw history,0.211366 for reset,
and0.111914 for rebasing. Its post-event16-frame mean is0.328261,0.143037,
and0.113901 respectively. Case76 shows the same ordering at the event:
0.506930,0.192164,0.060916. Reusing corrected observations beats discarding them
for these changes.

Raw rebasing has no per-clip mean FLIP or LPIPS regression in this24point
case/scale comparison. Cheap rebasing is a mixed-metric control: all clip FLIP
values improve or stay exact, but case77 native LPIPS rises5.036%, and its4×
six-case mean LPIPS is slightly worse. Raw reset's case77 at2× has30.495% worse
LPIPS than ordinary history. No method is promoted by FLIP alone.

**Followup1: eight physical history frames.** Retain all sixteen jitter phases
and the existing query-table allocation, but halve packed observation slots and
cap the causal window at eight. This saves20,736,000requested bytes and lowers
complete median to11.55872ms. At4×, however, FLIP worsens14.94%,LPIPS22.21% and
temporal residual92.18% versus sixteen-frame rebasing. Retire.

**Followup2: keep the sixteen-frame spatial footprint.** With eight stored frames,
use the old16-observation support radius once eight observations exist; the first
seven cold frames keep their original radius. Complete median falls to10.79756ms,
with the same71,277,084live bytes. FLIP is still9.47% worse, LPIPS26.39% worse,
and temporal residual115.77% worse than sixteen-frame rebasing. Retire.
The missing observations matter; merely changing the filter does not recover them.

The direct-source control averages0.002261FLIP and uses25,212,040live bytes at
3.57960ms on this fixture. Raw rebasing therefore remains expensive and inaccurate
against the strongest unrestricted authored-source control. Nor does0.116796
mean FLIP meet G1's0.03 target. No G4 memory compression or production advantage follows.

## Retained control, verification and evidence

The capture tool accepts --light-history 0|1|2 (keep/rebase/reset) for continuous
analytic raw-factor or the untrained source-aware history control. Default0
preserves prior behavior. Model/gate/training-input paths reject nonzero modes,
so they do not silently receive an unmeasured input. A future learned comparison
must explicitly receive the same useful event metadata. The public factor GPU
guard is retained in studio/reconstruct/light-factor-test.

Initial ordinary controls match1,536prior rendered frames exactly, with identical
targets/masks. For each new rebasing/reset method,640non-event/pre-event frames
are byte-identical to its ordinary control. After removing experiment constants
and exposing the option, six batched validation captures match **1,536frames**
exactly, plus their targets/masks; established quality scores are reused.
Format/type checks and executable selected paths pass. No broad unchanged-engine
audit or new training is performed.

Nine experimental batches capture6,912frames; six selected-code validation batches
capture1,536more. Each batch covers multiple cases/scales from one build.
Eleven complete profile/residency pairs plus two three-pair timing comparisons
are preserved. Failed source, rejected candidates and raw metrics remain available.
No capture-speedup ratio or infrastructure rendering win is claimed.

Pop!_OS evidence: build/neural-evidence/radiance-evolution-v1.
Manifest: `e345b435333a94d718a83de88a76cc78c7e93738faf72e4aef09387eb0309b96`.
The eight frozen files, including G1–G7 and existing models, stay unchanged.
Inherited benchmark edits remain unstaged.

**Next architecture-level bet:** source-generated observation records. Each packed
observation uses8bytes for half colour/tag and12bytes for f32motion/depth.
Try retaining all sixteen radiance phases while recovering redundant geometry
from authored source metadata and observed tags. Exact render-target rounding,
source-query cost, updates, scratch and resident memory must be measured together.
This is an untested representation hypothesis, not an assumed24.9MB rendering win.
