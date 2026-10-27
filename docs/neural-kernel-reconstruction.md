# Learning weights over retained renderer samples

Status: development experiment following the rejected joint RGB/history models at
`46f9c968`. No G1–G7 gate is claimed or changed. This note separates published
methods, our proposed architecture, and the measurements needed to accept it.

## Primary sources

Bako et al., [Kernel-Predicting Convolutional Networks for Denoising Monte Carlo
Renderings](https://la.disneyresearch.com/wp-content/uploads/Kernel-Predicting-Convolutional-Networks-for-Denoising-Monte-Carlo-Renderings-Paper33.pdf)
(ACM TOG 36(4), 2017, Section 4.2) compare direct RGB prediction with predicting scalar
neighbourhood weights shared across RGB. Softmax makes those weights nonnegative
and sum to one, bounding the output by the observations' convex hull. Their
application is static Monte Carlo denoising for film production; its results do
not establish temporal supersampling quality or browser performance for Wrela.

Yang et al., [MNSS: Neural Supersampling Framework for Real-Time Rendering on
Mobile Devices](https://discovery.ucl.ac.uk/id/eprint/10215223/) (IEEE TVCG 30(7),
2024) jointly consider sampling, a small reconstruction model, and accumulation
into a high-resolution history buffer. This motivates testing the information
retained by our sampling/history pipeline before enlarging our RGB decoder.
Their reported mobile results do not transfer to our AMD hardware or benchmark.
Neither paper's code, weights or data are copied.

## Wrela hypothesis

The engine can expose causal surface correspondence separately from expensive
appearance evaluation. Its authored fields can also generate exact diagnostic
targets, deliberate distribution shifts and independent reference checks.
This is useful only if those privileged targets remain outside inference and
the cost of every source query is included.

The previous patch MLP predicts RGB residual polynomials and repeatedly filters
its previous output. Rejected joint models also regress on new colours and
motion parameters. The next hypothesis is to retain original renderer
observations and learn their weights, reducing the need to predict RGB values
and avoiding repeated resampling of already filtered history.

Nonnegative normalized weights preserve constant observed colours. A stronger
claim, affine RGB equivariance, additionally requires that selection and weights
do not change under the colour transform and that clipping/quantization be
excluded or separately bounded. A colour-dependent weight network does not
automatically have that property. Unobserved detail and newly revealed colours
remain an information limit.

## First control

The `raw-source` experimental decoder keeps sixteen causal raw frames. The first
control used 32-byte colour/motion/depth records; the current cache uses 20 bytes,
keeping already-half-float colour in two packed words and retaining f32 motion
and depth. It gathers both authored layer trajectories, classifies samples by
stored depth and motion, and rejects samples whose projected point is occluded
at the current time. Surviving observations receive positive tent weights and
are normalized in scene-linear RGB.

The shader receives only slat period, width, offset and speed as extra source
parameters. It receives no source colour, checker frequency, coverage integral,
reference image, future frame or case identifier. This is a full-stack fixture
control, not a fixed-input upscaler comparison and not a general 3D solution.
Cuts must clear history; no-reset captures are deliberate negative controls.
Truth and visibility masks are produced separately after reconstruction.

The initial comparison is predeclared on legacy cases 1 and 6, all four scales,
32 cold-start frames at 192×108. Compare against the unchanged raw decoder and
the frozen source-geometry constant-history control, using identical reference
and mask hashes. Independently replay the actual exported observations on the
AMD CPU and test wrong correspondence/jitter negatives before attributing
quality differences to the algorithm.

## Promotion requirements and cost risk

A learned weighting experiment must use whole-case train/development splits,
the same held-out stress cases, recorded seeds and final model hashes. Its
gradient, CPU/GPU parity, cold starts, cuts, all scales, perceptual errors and
reference-subtracted motion errors must be checked. An average gain cannot
erase a tail regression. The public analytic family cannot satisfy the sealed
scene, independent renderer, human or production requirements.

The original history alone used 63.28 MiB at 1080p output / 4× linear reduction
and 253.13 MiB at 4K / 4×: sixteen 32-byte records per input pixel. The current
20-byte cache uses 39.55 and 158.20 MiB respectively at those dimensions; other
buffers are additional. Both retain all sixteen raw frames. This is packing of
observed half-float colour, not learned appearance compression or a G4 result.

The original two layers × sixteen frames × nine taps visited at most 288
candidates per output pixel. The current implementation restricts the loop to
a conservative support rectangle, then rejects impossible support before loading
storage or querying visibility. These are work/capacity descriptions; measured
latency and total fixture allocations belong in [the benchmark record](bench.md).
G6 still requires its original browser latency, memory and whole-frame gates.

## First learned kernel

The deployed experiment uses an 8→8 ReLU→1 MLP with 81 f32 coefficients and
three checked padding floats (336 bytes total). Its inputs are absolute
normalized x/y distance, sample age, available history count, inverse divisor,
layer, absolute horizontal velocity and radius/divisor. An exponential of the
logit clamped to [-5,5] multiplies the tent weight; normalization happens after
the learned weighting. Eligibility and the zero-support fallback depend on
geometry and base support, not the learned scale. A zero output head reproduces
the fixed control exactly.

Training uses 64 seeded random output pixels per frame across cases 0/3/5/7,
32 cold-start frames and all four scales: 32,768 current training positions.
Previous reconstructed positions for the opaque temporal loss are also
evaluated from raw causal observations; the exact full-coverage masks and
horizontal interpolation rule match the independent scorer. No reconstructed
output feeds another frame. Targets are training-only. Two fixed 2,048-step
Adam runs use named-view MSE plus temporal MAE coefficients 0 and 0.25.
Both final models are evaluated on legacy cases 1/6 and the six previously
declared stress cases; neither is selected using stress scores.

CPU differentiation, constant-colour and affine-colour controls, AMD GPU
zero-head parity, model upload rejection, trained cut/reset parity and
independent numerical audits are recorded with the experiment. The final
promotion decision and quality measurements belong in the development record.

## Follow-up hypotheses after the cost measurement

Two optimizations have been implemented and measured separately. First, reject a
candidate before its storage load when even the accepted motion discrepancy
cannot put it inside the tent. Second, invert that conservative support interval
to bound the old 3×3 search, preserving candidate order, and pack the history.
The margin includes 0.05 output pixels/frame times age plus coordinate-scaled
f32 roundoff. Removing the motion margin fails the bound tests. The actual
stored-motion, depth, visibility and tent checks remain.

Packing does not round motion to half precision. All 63,488 finite half-float
bit patterns round-trip in the AMD GPU probe, and the expanded 32-byte diagnostic
exports are byte-identical to the original cache. Export layout and internal
storage format are recorded separately. Sixty-four 20-byte records make each
frame's binding offset a multiple of 256 bytes.

The first pruning stage preserves every tested HDR byte. The packed/bounded
stage does not preserve every learned output byte: both isolated changes can
alter arithmetic rounding, while repeating the first stage reproduces its old
bytes. Validation therefore records the failed strict comparison and checks
at most one positive finite half-float code step per reconstructed RGB channel,
exact alpha/reference/mask bytes, exact cut/fresh resets, and newly computed
perceptual and temporal scores. Fixed-filter clips remain byte-identical.
This bounded numerical check does not change any G1–G7 tolerance and does not
promote either previously rejected model.

A more substantial design would cache a compact set of raw observations in
surface coordinates or an output grid and reconstruct from a small support.
That changes collision handling, visibility and sample-age accounting and
must be evaluated as a new algorithm, not assumed equivalent.

Fan et al., [Real-time Monte Carlo Denoising with Weight Sharing Kernel
Prediction Network](https://arxiv.org/abs/2202.05977) (CGF 40(4), 2021; arXiv
version 2022) predict a compact encoding of filtering kernels and decode it
with unfolding operations. This supports investigating reusable filter
representations. Their denoising result does not establish a speedup for our
temporal sample search; our measured cost includes finding and reading samples.

Li et al., [Neural Super-Resolution for Real-time Rendering with Radiance
Demodulation](https://arxiv.org/abs/2308.06699v2) (CVPR 2024) separate lighting
from material detail, super-resolve lighting and recombine it with a
high-resolution material component. This suggests another portfolio direction
for Wrela's authored fields. It adds information and work relative to an RGB
upscaler. Output-resolution material evaluation, cache storage and any neural
appearance decoding would be explicitly charged to the full-stack budget;
nothing in this experiment demonstrates that tradeoff yet.

## Spatial-index control

The explicit `raw-index` decoder keeps the same raw history and filter, but builds
an index in current output coordinates. Two-pixel bins have an eight-pixel halo,
large enough for the maximum cold-start support at all four diagnostic scales.
A 2D dispatch examines each retained observation once, checks layer/motion and
current source visibility, and inserts its node into a bin with an atomic exchange.
One link per allocated observation provides collision storage without a per-bin cap.
A separate clear pass removes previous-frame heads. Both passes are timed and charged.

Reconstruction visits only bins intersecting its tent, follows their lists and
retains the original 3×3 per-frame/per-layer candidate domain. It does not enlarge
the filter, discard crowded-bin entries or read source colour. The graph is rebuilt
from all live raw frames; this is not an incremental surface-space cache or a
general moving-camera/3D visibility solution. Diagnostic exports contain bin heads
then age-major links, with zero as null and node IDs encoded plus one.

Atomic insertion order is unspecified. The independent graph audit therefore
requires exact membership and bins, no duplicates or cycles, and no lost collisions;
it does not require identical link order. Repeated actual GPU runs change some
reconstruction channels by one half-float code step. This variation and its temporal
implications remain part of acceptance; no deterministic-output claim is made.
The analytic scores do not replace G3's trail/reveal/population requirements.

The measured two-pixel variant reduces median fixture span modestly but increases
memory and worsens some p95 results. It remains an explicit experiment, with the
previous decoder available. [The benchmark](bench.md#2026-10-27-utc-amd-spatial-sample-index)
charges clearing, indexing, gathering, retention and display separately.

Next hypotheses include smaller bins to reduce candidate traversal, a contiguous
index to avoid dependent pointer reads, and specialization of the kernel network.
Only normalized x/y distance varies per output/sample pair; the other six network
features are constant for an age/layer/scale/history-count/source-velocity combination. Folding
those features into hidden biases could reduce repeated work. This is an algebraic
proposal, not a measured speedup or a claim that deployed f32 arithmetic is unchanged.
Any variant still needs numerical/temporal checks and a complete cost comparison.


## Partially evaluating shared inputs

The opt-in `--raw-specialize 1` path supports both `raw-source` and `raw-index`.
For each of sixteen ages and two source layers, six MLP inputs are constant.
`edges/kernel_context.wrela` computes their hidden-layer contribution once per
frame, transposes the two varying distance coefficients into vector rows, and
copies the unchanged output head. Each contribution then evaluates only the
two distance inputs, ReLU and output head. All 81 coefficients remain the same;
there is no retraining or extra source appearance information.

The 288 vec4 rows use 4,608 bytes and are rebuilt as history count and scene
parameters change, including cold starts/cuts. Regrouping f32 arithmetic can
change half-float output by a step, so algebra alone is insufficient validation.
Independent table reconstruction, continuous clips, zero-head, repeat and
cut-reset controls are recorded in
[the development record](neural-rendering-progress.md#preparing-the-kernels-shared-geometry-terms).
Complete browser timing and extra memory are charged in
[the benchmark record](bench.md#2026-10-27-utc-amd-neural-kernel-context-preparation).

This is a useful example of source geometry exposing reusable neural terms,
but remains manual specialization of a two-layer analytic fixture. It does
not imply equivalent reuse across arbitrary motion, deforming surfaces,
lighting or production scenes. Both frozen models remain rejected on quality;
the explicit switch defaults off.
