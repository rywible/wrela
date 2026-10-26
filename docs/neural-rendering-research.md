# Neural rendering from authored fields

Research note, 2026-10-26. This is an architectural assessment of the current working tree,
including its uncommitted B14 experiment, and primary literature. Proposed capabilities below
are not implemented or measured unless explicitly identified as existing.

## Thesis

Wrela's strongest advantage is that the authored program remains an executable teacher for
every approximation. Its content already supplies structure that a renderer trained from
photographs would have to infer: geometry, gradients, material channels, part identity, rest
coordinates, poses, and scale. A promising direction is **learned compilation of expensive
field evaluations**, retaining source fields as the authority and choosing where to evaluate,
bake, cache, or approximate them.

Neither neural fields nor procedural supervision is new. Instant-NGP already trains SDFs from
generated geometry samples, and SIREN trains continuous functions and their derivatives. The
potential advantage is integrating those ideas with Wrela's authoring, realization, caching,
and measurement pipeline. It is an engineering opportunity, not an established uniqueness or
performance claim. [Instant-NGP](https://nvlabs.github.io/instant-ngp/),
[SIREN](https://www.vincentsitzmann.com/siren/).

## What the engine already provides

| Existing capability | Why it matters for learning | Repository evidence |
|---|---|---|
| A creature's rest-space field, skeleton, part identity, material channels, and derived gradients | Generate consistent targets directly; separate appearance from articulation | [`Creature`, `CreatureField`, `Parts`](../engine/creature.wrela); [`Look`, `Poser`](../engine/character.wrela) |
| Field bounds, part masks, intervals, and surface extraction | Spend teaching samples near relevant surfaces and semantic parts; retain an authored geometric fallback | [`Parts::cull`](../engine/creature.wrela); [`realize`](../engine/realize.wrela) |
| Derivatives with respect to authored literals in the studio | Supervise how a surrogate changes with meaningful edits, where derivatives exist | [`solve`](../studio/solve.wrela), `grad8`/`grad16` |
| Explicit footprints and several cooked representations | Learn scale-dependent answers and place decoding where work is reused | [`Look::surface`](../engine/character.wrela); [`Terrain::cover_seen`](../engine/terrain.wrela); [`lod`](../engine/lod.wrela) |
| Build-time shipped computation and a measured cook budget | Train or bake reusable products outside drawing, then budget refreshes at runtime | [`history::BAKED`](../games/last-green/history.wrela); [`cook::Planner`](../engine/cook.wrela) |

These facilities are not a generic differentiable renderer. The studio's literal derivatives,
for example, do not establish automatic differentiation through visibility, the entire
renderer, or arbitrary training loops. A field's distance value also need not be an exact
Euclidean distance everywhere; keep its actual bounds and semantics.

## Six opportunities

### 1. Train from the field, including its difficult regions

Use the authored field to generate samples of distance, normal, material, and part identity.
Concentrate samples near silhouettes, thin features, high curvature, joins, and locations
where a provisional model disagrees with the teacher. For an editable family, include
parameter derivatives where available, alongside values and spatial gradients.

The first advantage is data quality and control: there is no need to recover geometry,
material, lighting, or correspondence from final RGB images. A second is an active feedback
loop: the model can request more examples precisely where it fails. This is proposed use of
the existing `Parts` and studio derivative facilities. SIREN supplies precedent for derivative
supervision; its results do not imply that Wrela's current ReLU MLP has the same behavior or
cost. [SIREN](https://www.vincentsitzmann.com/siren/).

### 2. Reuse structure across instances and authored families

Query a decoder in an object's or part's canonical space. Supply meaningful morphology and
appearance parameters, or a small per-kind latent, rather than making it memorize world
position and every animated frame. Start with a family such as one tree species with varying
crown shape, leaf density, and colour. Preserve the existing pose machinery for motion.

Wrela already knows rest coordinates and skeleton transforms; learning them again would add
work. Canonical neural representations have prior art in SNARF, which also demonstrates why
inverting deformation can require multiple correspondences and root finding. Wrela's
existing surface meshes can carry rest coordinates directly; this does not make arbitrary
world-space inverse skinning trivial. An arbitrary random seed is not a useful continuous
morphology coordinate. Validate held-out individuals and poses, not just nearby samples of
the same object. [SNARF](https://openaccess.thecvf.com/content/ICCV2021/html/Chen_SNARF_Differentiable_Forward_Skinning_for_Animating_Non-Rigid_Neural_Implicit_Shapes_ICCV_2021_paper.html).

### 3. Make scale and filtering part of the learned function

Learn the answer for a pixel's footprint: filtered coverage, normal distribution, material
response, or transmittance at the requested scale. A continuous field can generate finer
reference samples for this integration. Train jointly across correlated channels instead of
treating colour, normal, and opacity as unrelated images. For foliage, silhouette coverage
and subpixel stability should carry explicit loss weight.

This fits Wrela's existing footprint-aware interfaces particularly well. Mip-NeRF shows why
point queries alone cause scale-dependent aliasing; neural appearance models demonstrate
filtered reflectance with learned shading frames; neural texture compression jointly encodes
material channels and mip levels. These support the direction, not a promise that averaging
roughness or normal components produces the correct filtered BRDF.
[Mip-NeRF](https://jonbarron.info/mipnerf/),
[Real-Time Neural Appearance Models](https://research.nvidia.com/labs/rtr/neural_appearance_models/),
[Random-Access Neural Compression of Material Textures](https://research.nvidia.com/labs/rtr/neural_texture_compression/).

### 4. Select representations using an explicit error budget

Treat a network as another representation alongside mesh, atlas, and volume. Compare its
answers with the teacher, measure image and temporal error, and select it only in the domains
where it passes. Retain a cheap analytic base or conventional representation for silhouettes,
visibility, out-of-domain inputs, and failed cases; let a network approximate the expensive
remaining detail. Error maps and held-out maxima can guide sampling and deployment.

Wrela has relevant machinery in field culling, LOD, and measured cooking. Its realizer measures
the field distance at mesh vertices and quad centres, explicitly noting that this cannot
detect a thin part missed by all grid corners. This is an existing design precedent, not
evidence that neural errors already have certified bounds. Finite sample tests or a network's
confidence score cannot certify sphere-tracing steps, collision, or the absence of missed
thin geometry.
[`realize`](../engine/realize.wrela), [`lod`](../engine/lod.wrela),
[`cook`](../engine/cook.wrela).

### 5. Decode at a place where the result is reused

Test a network that generates a material tile, impostor view, or volume brick once, with its
output cached for drawing. That uses the existing cook architecture and avoids multiplying
inference by foliage overdraw. Batch coherent queries and compare the whole path, including
refreshes and cache memory, with existing procedural cooking and compressed atlases.

This is a plausible early deployment strategy, not automatically a runtime-memory saving:
a decoded cache still consumes memory. Direct neural texture sampling instead trades shader
work against storage. Published neural appearance speedups rely on particular tensor
operations and implementations; neural block compression explicitly uses a tiled architecture
and hardware matrix multiplication. Wrela's current `eval_gpu` stores half weights but
unpacks them into scalar `f32` loops. Its present implementation must be measured on the actual
browser/device; half storage does not establish half arithmetic or tensor acceleration.
[`eval_gpu`](../engine/neural.wrela),
[Neural Appearance Models](https://research.nvidia.com/labs/rtr/neural_appearance_models/),
[Hardware Accelerated Neural Block Texture Compression](https://arxiv.org/abs/2506.06040).

NVIDIA's SDK explicitly separates inference on load, inference on sample, and inference on
feedback. Its guidance recommends inference on load for lower-end hardware; its direct
sampling guidance requires careful accounting for expensive inference and filtering. This
supports testing cached decoding first, without implying that its Direct3D/Vulkan hardware
paths are available in Wrela's browser renderer.
[RTXNTC SDK](https://github.com/NVIDIA-RTX/RTXNTC/blob/main/README.md).

### 6. Learn expensive light transport or where to spend samples

Longer term, use authored geometry and materials to create reference indirect-light samples,
then learn a low-frequency indirect-light approximation or a correction to the existing
probes. A narrower first experiment could predict which probe regions most need refresh.
Keep direct visibility and sharp contact relationships in conventional mechanisms until a
learned replacement demonstrates adequate reliability. Train across lighting and world
changes rather than baking one sun into the representation.

Neural radiance caching trains during path tracing to adapt to dynamic scenes; it is not a
drop-in replacement for Wrela's current probes. It requires training data, update work, and
an accounting of bias. Neural control variates offer a precise precedent for learning an
approximation and sampling the residual: their unbiasedness depends on retaining the
correction estimator. A generic learned residual or omitted correction is not automatically
unbiased. Neither paper's timings transfer to Wrela.
[Neural Radiance Caching](https://research.nvidia.com/labs/rtr/publication/muller2021nrc/),
[Neural Control Variates](https://arxiv.org/html/2006.01524).

## What the current impostor experiment establishes

The B14 record reports rung D at **13.86 dB** over all views against a **32 dB** gate. Its
**22.6 KiB** representation is much smaller than the **3 MiB** compressed atlas, but at
unequal image quality. In Chrome at 1080p on the M4 Air, sending every kind through the same
great-tree model produced incorrect images and a **390.0 ms** vegetation pass versus
**5.4 ms**; this is a stress measurement of that evaluator, not a valid visual comparison of
per-kind neural representations. The one-tree frame timings were inconclusive. These are
recorded measurements, not fresh measurements from this investigation.
[`docs/bench.md`, B14.2](bench.md).

The atlas is generated from the procedural source at load, not downloaded as a 3 MiB asset.
The current stage still cooks the kinds and creates `Wood` before `read_through` installs the
network; that method adds a weight buffer without disposing of the atlas. Consequently,
22.6 KiB versus 3 MiB is a representation-size comparison, not a measured reduction in total
GPU memory or download size. Procedural code is already compact: a neural representation must
also beat the existing source-to-cache cooker, and shipping weights can add download bytes.
[`stage`](../games/last-green/stage.wrela), [`Wood::read_through`](../engine/wood.wrela).

The source also exposes concrete tuning questions, independently of the recorded timings:

- The learned feature grid depends only on `texel_uv` and is shared across all 256 views.
  View dependence comes through two raw view-grid coordinates. Test view-conditioned or
  factored features, and compare with a representation using the authored object's structure.
- The training loss weights all eight channels equally, including empty texels' normal and
  depth targets. The reported score instead measures coverage-weighted colour plus coverage.
  Align the objective with rendered silhouettes, covered-surface attributes, and the score;
  measure each channel before attributing failure to capacity or training duration.
- Holding out every 16th view of a 16-by-16 grid holds out its last column. That is an edge
  extrapolation test, not a representative spread of unseen directions. Separate interpolation,
  extrapolation, and unseen-instance evaluations.
- Inputs have no footprint, and the network is evaluated at continuous coordinates while
  training samples are at texel centres. Evaluate filtering and motion explicitly.
- `cut_view` evaluates the complete eight-output network to obtain coverage, and `far_shade`
  evaluates it again for surviving shading. `eval_gpu` uses scalar `f32` arithmetic with
  repeated half-weight decoding. Test a cheap coverage path and amortized cache decoding before
  increasing the network's size. Actual savings require profiling.

These are code observations and proposed ablations, not established explanations for the
entire quality or timing deficit.
[`impostor training fixture`](../studio/fit/fixtures/impostor/main.wrela),
[`impostor sampling`](../engine/impostor.wrela), [`neural evaluator`](../engine/neural.wrela).

The current experiment therefore rejects that quality/cost point. It does not reject learned
compilation of fields. Before growing the network, investigate representation, sampling,
loss weighting, scale conditioning, and placement of inference. The next experiment should
compare matched visual quality, including thin silhouettes and motion, while reporting
training time, shipped bytes, peak/runtime memory, startup, and frame cost separately.

## Suggested first demonstration

Use one authored foliage family to teach a compact, footprint-conditioned representation of
coverage and material information. Compare direct decoding with decoding into a bounded
cache. Hold out views and individuals, include silhouettes and motion in evaluation, and
compare against the current compressed atlas at matched quality. If it wins, extend the
same teaching interface to terrain/material tiles; investigate indirect lighting after the
training and deployment costs are understood.

The desired result is an author writing a richer field while the pipeline chooses a measured,
lower-cost representation for the target device. That would exploit Wrela's full pipeline;
a tiny coordinate-to-atlas MLP explores only one narrow corner of it.
