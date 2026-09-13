# One next coat representation experiment

Status: **design only, implementation not authorized by this document**. Keep
`furStudy=0` and `coatStudy=0`. The actual 0306 patch and material-combination
images reject the current patch family; no further density, edge or nap tuning
is proposed. The accepted base Sunhare, face, motion and attachments stay intact.

## Decision and evidence

Test a continuous, thin fur volume over **only the existing Sunhare body part**,
represented by a bounded set of mesh shells and silhouette fins. The visible
question is whether connected coverage can replace the smooth flank/back and
hard contour together at 2.276 m. This is one representation experiment, not a
whole-creature coat or new rendering stack.

[Lengyel et al. (2001)](https://www.hhoppe.com/fur.pdf) sample a hair volume into
shells and sample that same volume in another direction for fins. Their paper
identifies gaps and excess transparency at grazing shell angles. This supports
testing both interior and contour cues together. It does not establish an M4
cost or guarantee that eight layers suffice. Our proposed single body chart is
a bounded alternative to their general lapped parameterization, not an
implementation of their complete method.

[NVIDIA's shell/fin sample](https://developer.download.nvidia.com/SDK/10/direct3d/Source/Fur/doc/FurShellsAndFins.pdf)
provides the same useful comparison: surface layers alone have an angular
representation limit. The native 0237 calibration established that our current
A2C path passes fractional coverage; the 0212/0306 images still show raised
islands over clay. Fixing alpha did not supply continuous coverage. The new
test must therefore change the represented domain, not conceal patch borders.

## Smallest reproducible source

Use seed 37011 and the exact default body field from `SanctuarySunhareDesign`.
Retain the original compiled base mesh as the opaque substrate. Generate one
axial chart for this body only: longitudinal Z and angle around its local
cross-section. Derive chart roots from the actual blended body field, and reject
any sampled ray with multiple surface exits or failed projection. Pole/seam
samples must share positions and density. This is an explicit restricted-chart
assumption; a failed chart is a finding, not permission to add a general atlas
system or change the body into an easier shape.

Author short deterministic guides rooted over the entire permitted body chart,
comb neck toward haunch, and sample their occupancy into a periodic local
128×128×16 R8 volume. The texture is a cache of code-authored guides, never an
image asset or hand-painted approval. Root density is measured per square metre;
the proposed dry depth is 8 mm, continuously fading to zero at the neck and
sole exclusions. No isolated rectangular patch boundaries. Use the same density
source for shell intersections and preintegrated fin slices. A seam mismatch,
chart stretch or density loss must be visible in a diagnostic before shading.

One fixed eight-shell candidate, plus a twelve-shell convergence reference,
tests whether the lower sample count loses optical depth or forms terraces.
These are two resolutions of one source, not independently tuned coats. Cap the
source proxy at 2,048 triangles and selected fins at 1,024 triangles: at most
17,408 added triangles for eight shells, 25,600 for the diagnostic reference.
Preserve surface projection error ≤1 mm. If both shape and cap cannot be met,
report the conflict rather than weaken accuracy. Cap total added recipe storage
at 3 MiB, four cached variants at 12 MiB, including texture/mips and mesh arrays.
These are proposed ceilings, not compiled counts or measured allocations.

## Required renderer handshake before implementation

Current `groomCoverageAt` consumes strand phase, longitudinal coordinate, duty
and tip variation. Its pulse integral is not a volume sampler. Kind13's guide
GGX and generic wet finish likewise do not supply volume attenuation. Do not
silently reinterpret existing groom coordinates or assign a new material kind
without registration and ownership agreement.

Root owns any new texture binding, pipeline or surface-fragment interface. Astra
would own the project body chart, density compiler, shell/fin meshes and tests.
The minimum study interface needs a bind-space chart coordinate, normalized
coat depth, a shared density cache and explicit shell-versus-fin selection.
No game/editor species switch belongs in the engine. Current ABI may be reused
only with an explicitly versioned study material contract; no ABI reuse is
assumed here. Draw base first, then the study geometry, using the same native
MSAA and depth path; do not introduce a ray marcher or secondary simulation.

Before a creature capture, compare the actual A2C resolve to a software reference
for one uniform slab and one correlated guide volume. At normal and grazing
angles, include four/eight/twelve numerical samples. For a homogeneous slab the
reference transmittance is `exp(-sigma * pathLength)`; layer alpha is based on
its represented path length, not a fixed alpha copied N times. The heterogeneous
guide reference integrates the same occupancy. Reusing correlated sample masks
can make stacked A2C coverage behave differently from independent compositing;
measure this explicitly. Do not use time-varying random masks to hide failure.
If native resolve cannot match within 5 percentage points of linear coverage,
stop before fur authoring and report the missing coverage/compositing capability.
Display PNG values require transfer inversion; HDR linear readback is preferable.

Filtering must average occupancy/optical depth over the pixel footprint and
preserve integrated density as layers become unresolved. Sampling a binary mask
at one point or independently fading every layer to zero is not sufficient.
Fins must fade into the shell volume without a doubled dark rim. Wetness in this
first test is a stated authored reduction in depth and increased guide grouping,
not simulated capillary transport or a glossy plastic coating.

## One native gate and stopping rule

Retain an exact saved study from the 0306 rejected combo as historical comparator;
make a new matched base/eight-shell/twelve-shell study, default anatomy and shared
softbox, 1920×1080, fixed exposure, wind zero, paused. Proposed saved modes are
`base`, `volume8`, `volume12`; **these controls are not implemented commands**.
Use actual front orbit π, quarter π+0.8 and side π/2, elevation 0.2, distance
2.276 m, dry and wet. Repeat quarter at 0.5 m and 4 m; use the existing six hop
times from `GROOM_REPLAY.md`, unchanged pose/brain, then short live camera travel.
Persist source/cache/shader hashes, exact controls, native PNGs and a played
motion-strip receipt. No promotion based on numerical coverage alone.

Reject smooth clay between coat islands, shell bands, a uniform halo, fins that
read as cards, comb seams, wet plastic, eye/neck contamination, detached volume
during hop, or shimmer. A body-only pass establishes that representation on the
body; it does not qualify face/ears/limbs or material transfer to membranes.

Only after identity passes, root measures a short matched live A/B including
normal cache updates. The existing provisional added-cost ceiling is GPU p95
≤0.20 ms and CPU p95 ≤0.05 ms; the shell candidate may fail it. Actual 0017
populated eye-level GPU median/p95/max was 25.576/32.937/37.668 ms, already over
16.667 ms. There is **no established populated-frame headroom**. Keep full
p95/max, update hitches, RSS, thermals and workload/source receipts; the older
15.199 ms downward-view number is not an eye-level budget. A studio pass cannot
promote this into the over-budget populated scene. If identity or cost fails,
close this single experiment with its evidence instead of broadening the coat.
