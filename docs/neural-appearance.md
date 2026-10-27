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
(SIGGRAPH 2023, ``4.1–4.5)](https://research.nvidia.com/labs/rtr/neural_texture_compression/assets/ntc_small_size.pdf)
jointly optimize material-specific quantized feature grids and a decoder.
Their finer grid concatenates four neighbouring feature vectors, while the
coarser grid uses bilinear interpolation. Positional phase helps reconstruct
detail beyond the grid resolution. These are useful hypotheses for our
multi-view attribute atlas; their texture results do not prove foliage
silhouette quality, unseen-view quality or performance on this AMD browser.

[Belcour and Benyoub, Hardware Accelerated Neural Block Texture Compression
with Cooperative Vectors (HPG 2025, ``3–4)](https://arxiv.org/html/2506.06040v1)
use block-compressed latent textures and organize inference by tiles/material.
Their reported runtime uses hardware matrix acceleration on different GPUs;
it cannot be transferred to this adapter. Grouping work and using hardware
texture decoding remain ideas to measure. We do not copy their code or weights.

[Khronos Data Format Specification v1.3.1, `19](https://registry.khronos.org/DataFormat/specs/1.3/dataformat.1.3.html)
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

## Next decision

Before integrating a decoder into production, test whether a representation
that shares information through the crown's source geometry can preserve the
normal/depth/silhouette relationship at the full memory budget. Independently
storing all views is expensive; a coordinate MLP alone is too lossy in these
experiments. Source-generated off-grid views and changed individuals must
validate any proposed sharing.

A view-page cache remains a possible runtime strategy, not an implementation
or success claim. Its decoded pages, compression/decode scratch, cache misses,
fallback and update spikes must all count. Compare against stronger conventional
compression and the engine's volume representation before claiming G4.
The current two individuals are public diagnostics; they do not constitute
G7's twenty held-out assets or its five-minute validated-build requirement.

![Fixed view 103: albedo, coverage and decoded normals for the bake, BC, shipped MLP and quantized grid](figures/neural-appearance-view103.png)

Each panel has 64×64 source texels, enlarged with nearest-neighbour scaling.
The last column is a CPU decode of the actual packed trial, independently
checked against the training implementation. It is not a GPU runtime result.
All 256 views, not only this illustration, are scored in the retained record.
