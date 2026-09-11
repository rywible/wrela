# Performance implementation and acceptance — 11 September 2026

This change implements the proposal's localized compiler, rendering and simulation
optimizations, direct procedural grass, object-stage culling, and a conservative
first leaf LOD. It preserves the shared art direction, native 1920×1080, 4× MSAA,
blade/leaf populations, cloud resolution and live sky refresh schedule.

## Implemented

- Cloud attenuation stays in optical-depth form. Two exponentials replace the
  transmission/fractional-power family, including the analytic out-of-volume case.
- Canonical edge intersections and analytic normals are shared by incident cells.
  Safeguarded roots keep a sign bracket and stop at a relative spatial tolerance.
  An interior QEF solution bypasses active-set enumeration; the exhaustive solver
  remains a randomized/pathological test oracle.
- Field values and analytic gradients share one traversal. Hard CSG chooses an
  active branch rather than averaging a crease. Conservative regional programs
  prune losing hard/smooth union branches only outside their complete value bands.
  Reference-extractor fallback use and cost are now retained in the mesh report.
  Its topology/count fields still describe the dual attempt when fallback is used.
- Terrain construction evaluates height and its analytic gradient together. The
  existing terrain tessellation and sample positions remain in place.
- Visibility and LOD selection traverse each batch once per pass. Reusable
  selection buffers hold 32-bit IDs into immutable instance tables. LOD history
  uses flat arrays attached to batches, including separate shadow selection.
- An object SIMD group culls 32 meshlets/patches and compacts survivors into a mesh
  payload. Only survivors launch mesh shaders. Frustum planes are normalized once
  per pass. Bounds use maximum axis scale and the actual bounded wind response,
  authored wind strength and quadratic height-dependent displacement.
- Grass uses 32-byte descriptions, spatially ordered into patches of at most
  12 blades. Mesh shaders emit five vertices and three triangles per blade directly
  to rasterization. The vertex-path comparison mode expands the same source without
  a persistent expanded mesh. Position, dimensions and angle remain Float; color
  has 10 bits per channel (maximum absolute quantization error 1/2046).
- Distant leaves retain every leaf's four boundary points, color and orientation.
  Two triangles replace four by removing only the 35 mm interior ridge. Selection
  includes bounds on the response-field Jacobian and the quadratic bend weight,
  plus the existing subpixel error budget and hysteresis. This is a first geometric
  leaf LOD, not a multi-level directional-opacity cluster representation.
- Wind uses persistent advection, pressure and response buffers and writes directly
  to the renderer's three immutable-in-flight upload slots. The collocated grid is
  retained for saved-state compatibility. Its pressure operator is now the actual
  composition of centered divergence and gradient (stride-two Laplacian), solved
  with weighted Jacobi. The manufactured divergent mode is removed by this
  compatible projection; finite iteration still leaves some other modes.
- Weather saturation returns its analytic slope from the same exponential. Clamped
  endpoints explicitly use zero slope. Transport uses persistent ping-pong cells.
  A bounded 64-entry checkpoint cache is keyed by exact seed, solar forcing and
  integer tick; switching presets and rewinding retain the existing replay semantics.
  A first uncached long-history rebuild remains synchronous and is not claimed fixed.
- Fully filtered material-noise bands return their exact mean before generating
  noise. HDR and bloom textures declare only their actual access uses.
- Profiling separately records simulation CPU time, atmosphere preparation/encoding,
  CPU encoding, submission-to-completion latency, GPU execution and OS presentation
  intervals. It records GPU core count. Missing presentation callbacks are reported
  as unavailable, not zero FPS. Atmosphere CPU time is a subset of encoding time;
  overlapping GPU stage intervals must not be summed.

## Evidence

Tests: full `swift test` passed 32 cases; release build and `--check-renderer`
passed on Apple M4. Workshop validation passed 17 checks and authoring validation
passed 19, including atomic rejection, frozen pixels, future wind replay, undo/redo,
file watching and shared style. A final targeted performance-math run also passed. Game validation passed its movement, GPU-field and capture checks. Invalid imported wind above the renderer response bound was rejected atomically with identical before/after pixels.

The machine reports an Apple M4 with **8 GPU cores**, no low-power mode and thermal
state 0 during the recorded runs. These are short measurements, not a sustained
thermal guarantee. Normal lighting, air and sky cache updates are included.

Compiler measurements are medians of three release-mode runs at resolution 64:

| Field | Complete mesh before | Complete mesh after | Reduction |
|---|---:|---:|---:|
| Sphere | 56.52 ms | 41.65 ms | 26% |
| Seed with cavity and stem | 180.40 ms | 113.81 ms | 37% |
| Blended canopy | 68.53 ms | 37.43 ms | 45% |

Triangle counts were respectively 36,732, 78,912 and 24,080 on both sides. Complete
mesh timing includes finishing/optimization and the seed's reference-extractor
fallback. The scalar evaluation counter covers the original explicit sampling
sites; it does not count every operation used to construct regional programs.
The benchmark harness and JSON are retained under `.soundstage/profiles/`.

Initial garden runs showed median CPU encoding 2.960 → 0.659 ms and median GPU
execution 12.675 → 12.637 ms. GPU p99 was 22.039 → 14.814 ms in those short runs.
They establish a CPU improvement, not a substantial median GPU speedup. These
initial runs had slightly different simulation ages; see the final matched runs
below. Metal allocation then changed from 698,761,216 to 694,468,608 bytes; whole
renderer allocation includes pipeline/resource overhead, so the blade descriptor
ratio must not be presented as a whole-renderer memory reduction.

Single-object Soundstage timing was mixed: the matched 15-second run's GPU median
was 6.20 → 8.03 ms, while p95 was 13.96 → 13.81 ms. No isolated-object GPU speedup
is claimed. Object-stage launch/encoding overhead and run variability remain costs
worth investigating independently of the measured garden CPU benefit.

### Final matched garden runs

Both apps loaded the same paused study at time zero, switched to the garden arrival
camera, warmed for three seconds and ran live for 20 seconds. The frozen original
app's source fingerprint matches repository HEAD `d132cc4` (its older revision label
predates that commit). Captures and final profiles use the same source settings.

| Metric | Frozen original | Final implementation |
|---|---:|---:|
| Median CPU encoding | 3.450 ms | 0.672 ms |
| Median GPU execution | 12.326 ms | 11.902 ms |
| GPU p95 | 13.855 ms | 15.504 ms |
| GPU p99 | 16.676 ms | 20.364 ms |
| Frame-interval p95 | 32.326 ms | 17.713 ms |
| Submitted FPS | 55.01 | 59.62 |
| Metal allocations | 704,217,088 B | 692,322,304 B |

CPU encoding is about 80% lower and the median GPU gain is modest (~3%). The final
run had **worse GPU tail percentiles**, despite steadier submission intervals;
short-run variability/cache-phase coverage does not establish a consistent GPU
latency improvement. This regression is retained in the report rather than removing
normal live update frames. Both runs reached simulation time 23.583 s. Published
snapshot times were [10.767, 16.133] before and [9.917, 14.883] after: no refresh-rate
or cache-age improvement is claimed. Different submission rates change cycle timing.
Final median simulation time was 0.00175 ms, p95 0.159 ms; the atmosphere CPU subset
had median 0.0733 ms. Neither number should be added to GPU time.

## Visual acceptance and known differences

The frozen comparison contains front, quarter, back, above, sunward, away and zenith
under noon, golden hour, sunset, afterglow, overcast, rain and softbox: 49 paired
conditions, 98 archived PNGs, exact settings, and source/shader fingerprints.
Baseline pixels were retained rather than re-rendered with the new code.

All 18 outdoor sky comparisons had maximum RGB-channel difference **1/255**; most
pixels were identical. Actual PNGs and representative native views were inspected,
including tree silhouettes, softbox foliage, arrival and meadow grass, wet material
and the style board. The HTML report and its metadata were inspected as well.

Compiler improvements change some field normals and the area-based leaf placement
that depends on the compiled canopy. The tree has the same broad shape and a full
canopy; individual leaves and tiny outline details are not pixel-identical. The
largest full-frame mean difference in the tree matrix was 2.50/255 (softbox above).
This is a numerical description, not a substitute for the silhouette inspection.
No exposure, palette or asset-source compensation was applied.

The procedural mesh and vertex paths were compared at a paused garden view:
99% of channel values were identical; mean difference including alpha was
0.0094/255. A small number of edge/depth-tie pixels differ between paths.

The low-sun live/reference pair used wind 2 and an 18.28-second-old snapshot. It
shows the existing approximate parallax/shape displacement relative to a fresh
render, with a 0.133-second reference delay. No single-depth silhouette tearing
was reintroduced. These existing aged-cache differences are not claimed solved.

## Remaining proposal work

The coarse global environment plus visible high-resolution sky tiles, sparse
potential bricks, local conservative cloud bounds/traversal, adaptive crack-free
terrain patches, richer coverage/opacity leaf clusters, packed general vertices,
light-space prefix integration, material caches, spatial cloud shadows and
secondary-ray lighting are **not implemented** here. They need their own complete
maintenance-cost and visual acceptance comparisons. The current spherical cloud
pullback, signed dense potential and coherent sky/irradiance publication remain.

Checkpoint reuse does not remove the first uncached hour-long weather replay.
Vegetation shader normals still do not include the full wind deformation Jacobian.
The current manifold flag continues to mean oriented edge incidence only.

## Local reports and fingerprints

- Frozen visual baseline: `.soundstage/studies/perf-before-20260911-084421-bec7b1/report.json`.
- Final paired visual report: `.soundstage/studies/perf-final-20260911-090503-de6711/index.html` (and `report.json`, `pixel-comparison.json`). This matrix precedes only the garden-specific terrain-normal and leaf-error-budget follow-ups; the sky shader and isolated tree are unchanged by those follow-ups.
- Style board: `.soundstage/studies/style-board-20260911-085532-8364f4/index.html`.
- Final garden profile: `.sanctuary/profiles/perf-final-matched-20260911-091255.json`.
- Matched original profile: `.sanctuary/profiles/perf-reference-matched-20260911-091515.json`.
- Matched arrival PNGs: `.sanctuary/captures/perf-final-arrival-11bcfd21.png` and `.sanctuary/captures/perf-reference-arrival-eca57551.png`.
- Live sunset motion: `.sanctuary/perf-sunset-motion.json`.
- Compiler timing: `.soundstage/profiles/compiler-before-after.json`; harness in `.soundstage/profiles/performance-evidence/`.
- Original source SHA-256: `0d43e8fe22bb1b79a22fd90197f69675502679482f697057fbb45fa27fd77e54`.
- Measured final garden source SHA-256: `839e3650452de1987830cdb855a401d0974b22eb2142676cf6fc73aa116d920f`.
- Final source SHA-256 (adds only the invalid-wind snapshot guard): `7deea4214632eb75a67d47dfaece00f3f4bd5b0eda9170aba31d3c0622db2b06`.
- Original shader SHA-256: `88302a05dc57e52a63418c9517ceb21d783d4783d5c593962f80ec219d6edc96`.
- Final shader SHA-256: `4200340e7ef1de1a1ef01212825dd9c0b82442a1604c162f6f95e829d0acf412`.

The original workshop session was saved as `perf-original-session.json`; named
checkpoints retain the pre-change and verified states. Assets and shared look were
not republished. No screenshots were synthesized to represent renderer output.
