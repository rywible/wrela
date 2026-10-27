# Learning weights over retained renderer samples

Status: development experiment following the rejected joint RGB/history models at
`46f9c968`. No G1–G7 gate is claimed or changed. This note separates published
methods, our proposed architecture, and the measurements needed to accept it.

## Primary sources

Bako et al., [Kernel-Predicting Convolutional Networks for Denoising Monte Carlo
Renderings](https://la.disneyresearch.com/wp-content/uploads/Kernel-Predicting-Convolutional-Networks-for-Denoising-Monte-Carlo-Renderings-Paper33.pdf)
(ACM TOG 36(4), 2017, `4.2) compare direct RGB prediction with predicting scalar
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

The `raw-source` experimental decoder keeps sixteen causal raw frames, using
32-byte colour/motion/depth records. It gathers both authored layer trajectories,
classifies samples by their stored depth and motion, and rejects samples whose
projected point is occluded at the current time. Surviving observations receive
the existing positive tent weights and are normalized in scene-linear RGB.

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

The uncompressed history alone is 63.28 MiB at 1080p output / 4× linear reduction
and 253.13 MiB at 4K / 4×, calculated from sixteen 32-byte records per input
pixel. Other buffers are additional. Two layers × sixteen frames × nine taps
is at most 288 candidate visits per output pixel, including rejected visits.
These are capacity/work counts, not measured latency. This deliberately costly
control must establish whether better sample handling helps before investing
in a compact cache, learned support or optimized GPU kernel. Deployment still
requires the original G6 browser latency, memory and whole-frame gates.

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

The current shader loads all nine neighbours for each layer/frame before testing
the spatial support. A conservative support test before storage loads is an
implementation opportunity, not a measured speedup. It must preserve the
existing accepted motion tolerance: the source trajectory and the stored
half-precision motion can differ. Rejecting from the source trajectory alone
can drop valid observations. Any such optimization needs byte comparisons of
cold, warm, moving, edge and cut sequences before performance measurement.

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
