# Neural appearance: authored foliage

This G4/G7 development track starts from actual authored foliage, after the
sample-kernel experiments. All measurements run on the Pop!_OS AMD machine.
No frontier gate is passed or changed.

## What the current representation loses

The shipped great-tree model takes view coordinates and local texel coordinates,
plus eight features from one 32×32 grid shared by all 256 views. Its grid cannot
store independent fine detail for each view; the decoder must express those
differences from its two view inputs. This is an architectural observation,
not proof that the grid alone causes its errors.

`studio/appearance` exports the exact existing bake, actual GPU-decoded BC7/BC5
atlas, actual GPU-decoded shipped network, compressed blocks and model bytes.
The target is the existing uncompressed 64×64-per-view bake. It is **not**
independently converged source geometry/light-transport truth. The current
mode-6 BC7 encoder is an available control, not a demonstrated best encoder.
The source includes normals, toward and depth; toward is a view-space position,
not directional transmission.

The instrument's CPU verifier reconstructs BC7 bits, BC5 endpoint/index data and
the half network independently. It reports per-view coverage, albedo, normal,
toward and depth errors, rather than allowing one colour score to hide failures.
No production rendering code or shipped weights change.

## Primary-source guidance

[Vaidyanathan et al., Random-Access Neural Compression of Material Textures
(SIGGRAPH 2023, sections 4.1–4.5)](https://research.nvidia.com/labs/rtr/neural_texture_compression/assets/ntc_small_size.pdf)
jointly optimize material-specific quantized feature grids and a decoder.
Their finer grid concatenates four neighbouring feature vectors, while the
coarser grid uses bilinear interpolation. Positional phase helps reconstruct
detail beyond the grid resolution. These are useful hypotheses for our
multi-view attribute atlas; their texture results do not prove foliage
silhouette quality, unseen-view quality or performance on this AMD browser.

[Belcour and Benyoub, Hardware Accelerated Neural Block Texture Compression
with Cooperative Vectors (HPG 2025, sections 3–4)](https://arxiv.org/html/2506.06040v1)
use block-compressed latent textures and organize inference by tiles/material.
Their reported runtime uses hardware matrix acceleration on different GPUs;
it cannot be transferred to this adapter. Grouping work and using hardware
texture decoding remain ideas to measure. We do not copy their code or weights.

[Khronos Data Format Specification v1.3.1, section 19](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html)
describes BC4/BC5 endpoints and interpolation indices. Our independent ideal
decoder and the actual AMD decoder differ in interpolated values. An exhaustive
controlled probe identifies a six-bit-weight replay for this device/environment;
that observation is not asserted to be a portable format definition or a
conformance judgment.

## First compression trial

The external `appearance-grid-v1` experiment gives each of the 256 views a
16×16×2 fine grid and an 8×8×2 coarse grid. Fine corners are concatenated,
coarse features interpolated, and four local phase values and two view
coordinates feed a 16→32→32→8 ReLU MLP. Latents use fifteen signed levels in
four-bit storage; weights are quantized to half during fitting. This is our
small architecture inspired by the feature-pyramid idea, not a reproduction
of the paper.

The fixed 4,096-step run fits all source-atlas texels. Its 85,648-byte payload
includes 81,920 latent bytes and 3,728 weight bytes. Caches, scratch, metadata
and conventional fallback would be additional. No resident-memory reduction
or GPU/runtime benefit is claimed from payload size alone.

This trial uses all 256 views during fitting, unlike the shipped model's
every-sixteenth-view holdout. Its atlas reconstruction is an asset-compression
diagnostic, not a fair unseen-view comparison. The original G4 requirement for
unseen views, lights, individuals, footprints and motion still applies.

[The measured record](neural-rendering-progress.md#authored-foliage-audits-and-a-rejected-quantized-grid)
contains results and failures. The trial improves the training reconstruction
but still loses coverage and grossly mispredicts normals. It is rejected.
A normal/depth error cannot be repaired by reporting colour PSNR alone.

## Source material captures between stored views

`studio/appearance` now has a `source` command:

```bash
wrela tool appearance --timeout 900 --blob-budget 3221225472 -- source 7 0.5 0.4375 64 0 64
```

Append `--pixel X Y` to also export that pixel's individual, unquantized
spatial samples for an independent accumulator check.

The positional arguments are seed, hemi-octahedral view coordinates, output
size, card level and spatial grid width. This example renders the full card
set at an angle between atlas views, at 4096×4096, then box-averages 64×64
samples into each of the 64×64 output pixels. Level 2 selects the bake's
quarter-density card set with twice the card width. Both levels use the
same bounds, material parameters, limbs and camera.

The source is cooked directly from the authored plant and rerasterized. It
does not read either the compressed atlas or neural predictions. It retains
the existing finite leaf-pattern texture and the bake's hard leaf cut,
crown normal, flat bark and quantized material targets. This isolates
geometry, view sampling and representation errors. It is **not** the complete
production leaf/bark shader or a converged light-transport reference. Wind,
lighting, temporal filtering, lower-hemisphere views and the full G4 footprint
range remain untested here.

`source.f32` contains twelve little-endian floats per pixel: linear albedo
times coverage (RGB), coverage, decoded world-normal times coverage (XYZ),
normalized toward times coverage, normalized crown depth times coverage,
and three reserved zeros. Normals are decoded before integration.
The final accumulator rounds each component to Q20, with nearest ties toward
positive infinity (at most 2^-21 absolute error per sample), and adds it
exactly using two integer words. It converts the final signed magnitude to
f32 and divides by the sample count. This precision is part of the reference
definition, not an unreported model error. Empty samples contribute zero to
every moment. Averaged normals are deliberately
not normalized: their length carries the variation within a pixel. These
moments cannot reproduce arbitrary nonlinear shading after averaging.

The fixed public diagnostic uses four atlas directions and four directions
between them, two card-placement seeds, both card levels, and 8×8, 16×16
and 32×32 spatial grids. The convergence limits were recorded before
capture. The initial grids did not pass; larger grids use the same limits.
A naive f32 accumulator also failed a physical normal-length invariant.
A compensated-summation trial retained its arithmetic in the generated WGSL
but returned exactly the naive results on this environment. The cause of
the unchanged GPU output is not established. The exact integer
accumulator is independently checked against CPU int64 sums and f64 means;
the failed trials and their source snapshots remain in the evidence.

A matching atlas-direction control checks the material adapter;
a high-resolution direct export independently checks the GPU's box reduction.
Repeated captures and wrong-orientation/wrong-view controls check the data
path. Results are recorded in `neural-rendering-progress.md`; an unconverged
case is retained as such rather than relabeled as truth.

The repaired 32×32→64×64 check passes all 32 fixed seed/view/level cases.
The coverage MAE limit is 0.001, coverage p99 absolute-error limit 0.01,
linear-albedo moment MAE limit 0.0005 and normal-vector moment MAE limit
0.002. Passing this finite sampling check establishes a useful material
diagnostic at this footprint; it does not establish general convergence of
production rendering.

![Full and coarse source crowns at a fixed off-grid angle](figures/neural-appearance-source.png)

## Batched source corpus and two cooperating models

Capture a fixed range of hemisphere views with one build/load:

```bash
wrela tool appearance --timeout 1800 --blob-budget 3221225472 -- source-grid 7 64 0 64 0 256 --bc
```

The arguments are seed, output size, card level, spatial grid, first view
and count. View i uses ((i mod 16 + 0.5)/16, (floor(i/16) + 0.5)/16).
Each view has its own directory. Optional `--bc` requires a size divisible
by four and exports actual GPU-decoded material moments and raw compressed
blocks. `area-bc.txt` identifies the layout: BC7 colour holds sqrt conditional
linear albedo times coverage and coverage; BC7 geometry holds mapped mean
normal XYZ and one; BC5 holds conditional toward and crown depth. Decode
caps mean-normal length at one without forcing unit length.

The full 256-view source corpus passes the same 32²→64² convergence checks.
A fixed two-model trial learns view-local coverage/toward and a shared 3D
material field conditioned on reconstructed position and viewing direction.
Quantized latents plus half weights occupy 161,522 bytes before runtime
overhead. The independently replayed model remains rejected: it loses to
the 3 MiB source-integrated conventional control. Source, fitting, model
bytes and all comparisons are in the
[measured record](neural-rendering-progress.md#source-integrated-visibility-and-shared-material-field-trial).

Offline depth reprojection helps both representations at unseen camera
directions. Moving conventional toward into an independent BC5 channel
also improves the control without increasing its payload. After one
reprojection step the neural trial still has 41% higher coverage error and
87% higher crown-depth error. Public validation on four between-view
directions and a second placement seed is useful for development, not a
sealed G4 or G7 result. No production model or shader changes.

![Stored and unseen directions: source, stronger BC control, rejected neural field and errors](figures/neural-appearance-field.png)

## Next decision

Visibility and position remain the limiting errors in this trial. Increasing
a material decoder's capacity alone would not address the observed coverage
loss. The next representation should be conditioned on authored geometry
and the pixel footprint, and should preserve position/coverage together
under view changes. The view-facing source cards themselves change with
camera direction; a single fixed 3D surface is an approximation. Test that
assumption explicitly before treating a mean position as sufficient.

The source pipeline can generate these targets and controlled variations,
but the complete production material, lights, wind, motion, unseen assets,
footprint range and strongest conventional alternatives are still required.
A view-page cache remains a possible runtime strategy; decoded pages,
scratch, misses, fallback and update spikes must all count. Neither the
packed payload nor this CPU fit satisfies G4's resident memory/timing
requirements or G7's twenty-asset validated-build requirement.

![Fixed view 103: albedo, coverage and decoded normals for the bake, BC, shipped MLP and quantized grid](figures/neural-appearance-view103.png)

Each panel has 64×64 source texels, enlarged with nearest-neighbour scaling.
The last column is a CPU decode of the actual packed trial, independently
checked against the training implementation. It is not a GPU runtime result.
All 256 views, not only this illustration, are scored in the retained record.
