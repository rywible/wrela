# Source-routed boundary decoding loses the complete tradeoff

2026-10-29 UTC. Parent `1d9e3cbc357541e567c82b6a9b0ca6e6c989179c`.

**Decision: retire the hybrid after two targeted followups.** Source-generated
boundary draws preserve the isolated-history variant's 768 output frames exactly
and reduce its measured complete cost to 9.68640 / 7.68244 ms on the two fixtures.
That is useful evidence about scheduling, not a new rendering win. The hybrid
still needs 108,601,884 live requested bytes, is substantially less accurate than
full raw-factor decoding, and is far slower than the cheap path for modest global
quality gains. The unrestricted direct-source control remains faster, smaller
and much more accurate on these supported fields.

First joint quality/runtime/residency assessment: **12.26 minutes**.
Retirement: **38.23 minutes**.
All runtime is restored exactly to parent. Failed builds, source variants,
captures and measurements are preserved. No training, new reference algorithm,
goal changes or G1–G7 qualification.

## The source-informed experiment

The [preceding diagnosis](neural-temporal-reference.md) found much lower
source-relative error from raw-factor decoding on pixels where temporal shortcuts
changed boundaries. This experiment tests whether the expensive decoder can be
restricted to source-known mixed-visibility pixels.

The initial `source-routed` fragment shader evaluates authored foreground
coverage at the output pixel. Fractional coverage uses the existing raw-factor
decoder; zero or full coverage uses the cheap source-aware bilinear history path.
Both receive period, width, offset and velocity plus the original incoming colour
samples. No source RGB, reference buffer, future frame, case-specific error mask
or diagnostic disagreement map enters routing. The resulting hybrid image is also
the next frame's cheap-path history.

It retains the complete 16-frame packed raw observations and source query tables.
Their updates execute even for pixels ultimately taking the cheap path.
All buffers, feature/preparation work, decoding, routing and final display work
are included. The ordinary functions extracted from existing fragment entry
points change none of the three controls' **2,304 output frames**: all are
byte-identical to their previous captures, with identical targets and masks.

The first comparison fails on both cost and quality. Complete medians are
11.04128 / 18.02448 ms versus full raw-factor's 10.79072 / 17.91200 ms.
Although mixed-boundary colour improves, several whole-clip metrics regress badly:
case 77 at 4× has 21.478% higher FLIP than the cheap path. Optimizing only draw
scheduling would leave that failure in place.

## Two consequential followups

**1. Isolate cheap recurrent history.** `routed-isolated` renders the cheap
path into its own current/last pair, then selects the final cheap-or-raw output in
a separate pass. One extra full-resolution RGBA16F texture adds **16,588,800 bytes**
at 1080p. This prevents refined boundary colour from feeding back into the cheap
history. Quality regressions shrink, but complete cost rises to
11.53628 / 18.38216 ms and requested live storage to 108,601,884 bytes.

Two binding/type errors during implementation are preserved: a selected temporary
cannot be bound as a texture place, and a non-Copy texture cannot be moved out of
borrowed state. Explicit branch-local binds repair them. They produced no GPU
measurements and are not optimization results.

**2. Derive boundary work from source edges.** The intended compact compute
domain addressed one source edge and output row per invocation, but Wrela rejects
constructing a remapped `GlobalId` for a texture write (`E0601`).
The rejected source and log are archived. No execution or speed is attributed to it.

The executable repair, `routed-work`, uses source-generated one-pixel boundary
draws. It first copies the independently maintained cheap image, then draws only
columns crossed by source edges. Consecutive edges landing in the same column
are deduplicated; outside/duplicate instances become degenerate primitives.
The fragment shader retains exactly the initial fractional-coverage predicate
and raw decoder. No mask texture, list, counter, sort or queue is introduced.
All draw generation, copying and overdraw costs remain included.

This is the second scheduling experiment, repaired onto a supported execution
path, not a third optimization. A separate prepared columns helper was never run.
The final image is byte-identical to isolated history for **all 768 frames**.
The recorded quality scorer had already finished before an intended metric-reuse
shortcut was installed, so metrics were independently recomputed; the subsequent
equivalence record correctly reports that no metric files were reused.

## Complete fixture cost and actual residency

Pop!_OS, AMD RADV RENOIR PCI 1002:164c / Ryzen 5 5500U, Mesa 26.1.6,
Chrome 154.0.8037.57, Wrela 0.9.0/compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
All timing points use 1920×1080 output, 4× reconstruction, one worker, 120 measured
frames after 60 warmup. Case 1 is flat radiance; case 74 uses nonlinear production
leaf material response. Separate 300-frame allocation runs use the same compiled
builds. The AMD queue is sequential; quality scoring follows measurement.

| Path / case | Saturated complete median / p95 ms | Paced complete median / p95 / p99 ms |
|---|---|---|
| fastsource-history / 1 | 2.00416 / 2.31672 | 2.25036 / 2.42184 / 2.55536 |
| fastsource-history / 74 | 1.85176 / 2.08660 | 2.37614 / 2.67623 / 2.74077 |
| raw-factor / 1 | 10.79072 / 12.06388 | 11.07883 / 12.54866 / 13.45522 |
| raw-factor / 74 | 17.91200 / 22.59396 | 17.92370 / 22.63904 / 22.78613 |
| source-direct / 1 | 1.78300 / 1.88380 | 1.87652 / 2.01922 / 2.09398 |
| source-direct / 74 | 2.25964 / 2.28892 | 2.86801 / 3.18586 / 3.25130 |
| source-routed / 1 | 11.04128 / 12.30844 | 11.08078 / 12.29485 / 13.54911 |
| source-routed / 74 | 18.02448 / 23.07776 | 18.05782 / 23.11869 / 23.24069 |
| routed-isolated / 1 | 11.53628 / 12.75448 | 11.74197 / 13.06933 / 14.03407 |
| routed-isolated / 74 | 18.38216 / 23.15464 | 18.39830 / 23.18328 / 23.46350 |
| routed-work / 1 | 9.68640 / 10.42148 | 10.02587 / 10.88129 / 10.99901 |
| routed-work / 74 | 7.68244 / 8.20172 | 8.08896 / 8.45455 / 8.77543 |

Saturated medians/p95 come from `profile.json`; paced median/p95/p99 come from
the separate replay's timestamp spans. These distributions are not combined.
These are unpaired development measurements without a power/thermal lock; no
confidence-qualified small speedup or sustained presentation result is claimed.

| Path / case | Requested live / peak B | Actual resident VRAM / GTT B | Browser PSS B |
|---|---|---|---|
| fastsource-history / 1 | 48,021,584 / 56,315,984 | 86,204,416 / 18,427,904 | 593,053,696 |
| fastsource-history / 74 | 48,021,584 / 56,315,984 | 95,121,408 / 18,427,904 | 594,884,608 |
| raw-factor / 1 | 92,013,084 / 100,307,484 | 145,031,168 / 18,427,904 | 589,014,016 |
| raw-factor / 74 | 92,013,084 / 100,307,484 | 136,110,080 / 18,427,904 | 591,344,640 |
| source-direct / 1 | 25,212,040 / 33,506,440 | 69,849,088 / 10,133,504 | 592,310,272 |
| source-direct / 74 | 25,212,040 / 33,506,440 | 69,853,184 / 18,427,904 | 594,258,944 |
| source-routed / 1 | 92,013,084 / 100,307,484 | 145,039,360 / 18,427,904 | 588,759,040 |
| source-routed / 74 | 92,013,084 / 100,307,484 | 136,118,272 / 18,427,904 | 590,370,816 |
| routed-isolated / 1 | 108,601,884 / 116,896,284 | 161,910,784 / 18,427,904 | 590,722,048 |
| routed-isolated / 74 | 108,601,884 / 116,896,284 | 152,989,696 / 18,427,904 | 590,488,576 |
| routed-work / 1 | 108,601,884 / 116,896,284 | 161,902,592 / 18,427,904 | 589,834,240 |
| routed-work / 74 | 108,601,884 / 116,896,284 | 161,902,592 / 18,427,904 | 555,517,952 |

Requested accounting includes raw observations, histories, source tables, output,
scratch and unused common fixture resources. DRM snapshots are deduplicated by
device/client; PSS covers the browser process group. These are actual whole-client
resident observations, not per-component physical peaks. Do not sum overlapping
UMA pools. The two independent-history variants add a real final-output texture;
the source-draw repair removes no persistent storage.

These are **complete analytic-fixture frames**, not complete production clearing
frames. They do not certify wood, 3D visibility/sorting, or production compositing
cost. No G4 memory reduction or whole-engine win is inferred. The fixtures are
sufficient to reject these candidates before expensive production integration.

## Quality and tail failures

Six existing public cases (1, 6, 13, 23, 74, 77), 192×108, four scales and 32 cold
frames are batched per executable. Shaded references and visibility targets are generated after reconstruction
and never bound as decoder inputs. Paired comparisons verify identical references,
masks and metric conditions. The following are equal-case means. Native-scale,
per-clip, cold/late, reveal and visibility-change data are also retained.

| Path | Scale | Mean FLIP | Mean LPIPS | Mean PSNR dB | Temporal residual | Mixed-boundary RGB MAE |
|---|---|---|---|---|---|---|
| fastsource-history | 2 | 0.255174 | 0.359763 | 17.793354 | 0.012052 | 0.087923 |
| fastsource-history | 3 | 0.294834 | 0.543333 | 16.724787 | 0.015828 | 0.103538 |
| fastsource-history | 4 | 0.319201 | 0.585481 | 16.128564 | 0.016180 | 0.113753 |
| raw-factor | 2 | 0.103125 | 0.084075 | 23.823028 | 0.009876 | 0.024916 |
| raw-factor | 3 | 0.139618 | 0.217954 | 20.823458 | 0.012508 | 0.037844 |
| raw-factor | 4 | 0.170361 | 0.260619 | 19.625425 | 0.013622 | 0.044583 |
| source-direct | 2 | 0.001711 | 0.000003 | 82.767878 | 0.000014 | 0.000081 |
| source-direct | 3 | 0.001711 | 0.000003 | 82.767878 | 0.000014 | 0.000081 |
| source-direct | 4 | 0.001711 | 0.000003 | 82.767878 | 0.000014 | 0.000081 |
| source-routed | 2 | 0.256753 | 0.229790 | 18.202806 | 0.012200 | 0.024916 |
| source-routed | 3 | 0.292544 | 0.450005 | 17.088724 | 0.015758 | 0.037844 |
| source-routed | 4 | 0.312216 | 0.484873 | 16.567801 | 0.016223 | 0.044583 |
| routed-isolated | 2 | 0.249154 | 0.271075 | 18.348042 | 0.012054 | 0.024916 |
| routed-isolated | 3 | 0.289926 | 0.467264 | 16.987965 | 0.015831 | 0.037844 |
| routed-isolated | 4 | 0.306529 | 0.498105 | 16.446606 | 0.016183 | 0.044583 |

`routed-work` has exactly the same quality as `routed-isolated`.
At 4×, the final hybrid improves mean FLIP from cheap history's 0.319201 to
0.306529 and LPIPS from 0.585481 to 0.498105, but raw-factor reaches
0.170361 / 0.260619 with less storage. Direct source reaches 0.001711 FLIP;
it evaluates authored radiance and its query work is included in its timing.
Its additional appearance access is disclosed; neither its quality nor its speed
waives the sixteen-times-fewer-colour-samples requirement or proves fixed-input
upscaler superiority.

Mixed-boundary RGB error matches raw-factor by construction, but neither this
diagnostic nor final opaque alpha certifies geometric silhouettes. Better selected
boundary error does not imply better whole-clip appearance.

| Variant | Worst clip FLIP increase at 2× | At 3× | At 4× |
|---|---|---|---|
| source-routed | 55.859% | 25.508% | 21.478% |
| routed-isolated | 9.654% | 12.143% | 8.428% |
| routed-work | 9.654% | 12.143% | 8.428% |

The final 3× case-77 FLIP increase is still 12.143%, beyond the charter's 10%
per-clip comparison allowance; no qualified candidate is promoted. Native-scale
case 77 also remains worse. At 4× the smaller worst-clip increase does not establish
a tournament or deployment pass: total cost, memory, broader scenes and confidence
requirements remain unmet.

## Evidence and next architecture bet

Six batched captures produce **4,608 output frames** in 148.76 seconds of
recorded capture invocation wall time, excluding the twelve profile/residency pairs
and CPU scoring. No unbatched speedup ratio is claimed. Existing checks, exact
control comparisons and actual GPU runs are reused; no broad unchanged-engine
suite or paired promotion study was run for a rejected architecture.

Pop!_OS evidence: `build/neural-evidence/source-routed-v1`.
Manifest: `c4e10affcb72a4dee70b4351a9815a41a9b6742d26cc8185d34b8389308adfa4`.
It pins the parent, three executed variants, failed compile sources/logs, all six
external capture roots, twelve runtime/residency pairs, scripts and frozen files.
Inherited benchmark edits remain unstaged.

**Next test source-declared radiance evolution.** The
[leaf diagnostic](../studio/reconstruct/edges/leaf_radiance.wrela) declares a
global intensity change from 1 to 0.35 at tick 16 while sun direction continues
to change. At fixed geometry/direction, its scaled sun/sky/reflected terms enter
[the material response](../engine/impostor.wrela) linearly. This suggests rebasing
causal colour observations with the known lighting factor before reconstruction,
equally in all conventional and learned comparators.

First verify that limited factorization on AMD, then compare rebasing to ordinary
history and explicit reset, including the transition frames and complete cost.
It does not solve changing direction, visibility or arbitrary material edits.
No target pixels, source RGB evaluation, hidden fitting or new material-moment
training is justified by this hypothesis.
