# Sanctuary: constraint-grown biome research

September 12, 2026. Bounded CPU experiment by Astra. **Research maps, not renderer evidence. No production terrain, save, shader, asset or placement changes.**

The experiment supports a constraint-first geography source: coherent uplift bands establish divides; drainage establishes catchments and retained basins; prevailing moisture transport and elevation influence habitat suitability. It does **not** support copying this raster directly into Sanctuary. Its visible grid-aligned channels, arbitrary soil coefficients, overly dry coverage and unfinished basin policy need another bounded study before a Swift production implementation.

## Questions and primary research

| Concrete risk | Primary source | Decision for this experiment |
| --- | --- | --- |
| Independently placed mountain bumps do not establish a drainage structure. | Cordonnier et al., [Large Scale Terrain Generation from Tectonic Uplift and Fluvial Erosion](https://cs.purdue.edu/homes/bbenes/papers/Cordonier16CGF.pdf), CGF 35(2), 2016, pp. 165–175. | Use coupled uplift and stream-power relaxation. The paper forms stream trees, connects overflowing lakes and solves erosion over a planar graph. This prototype uses a simpler raster and six bounded passes; it does not reproduce the paper's irregular graph, convergence process or terrain reconstruction. |
| Local downhill routing stalls in pits and can disconnect water across a saddle. | Barnes, Lehman and Mulla, [Priority-Flood: An Optimal Depression-Filling and Watershed-Labeling Algorithm for Digital Elevation Models](https://richard.science/sci/2014_depressions.pdf), Computers & Geosciences 62, 2014, pp. 117–127. | Compute spill levels with a deterministic priority queue. Keep spill water separate from terrain bed. Use steepest downhill D8 edges, with flood-parent ordering through flats, then audit acyclicity and outlet area. The paper distinguishes filling from breaching; a filled drainage representation alone is not proof that a lake actually holds enough water. |
| Rain based only on height or positive slope misses downstream transport and lee evaporation. | Smith and Barstad, [A Linear Theory of Orographic Precipitation](https://training.weather.gov/wdtd/courses/woc/winter/physiogeographic/cool-season-orographic/story_content/external_files/Smith%20and%20Barstad%202004.pdf), JAS 61, 2004, pp. 1377–1391. | Carry finite vapor, cloud and rainwater reservoirs downwind; include delayed conversion/fallout and descent evaporation. The paper also models airflow response and horizontal scales. This experiment does not implement its Fourier transfer function, buoyancy waves, real thermodynamics or calibrated precipitation. Its conservation check applies only to the synthetic reservoirs. |

The first paper was read from its author-hosted PDF, including stream construction, stream-power and lake-overflow sections. The other two PDFs were read for algorithm/transport assumptions. Soil retention and species response curves below are explicitly authored hypotheses, not scientific claims derived from these papers.

## Experiment and retained evidence

Staging root: [.build/sanctuary-native-20260912/constraint-biome-experiment](../../.build/sanctuary-native-20260912/constraint-biome-experiment/).

Final source is [experiment.py](../../Tools/Experiments/sanctuary_biomes/experiment.py) plus [compare.py](../../Tools/Experiments/sanctuary_biomes/compare.py), copied unchanged after the root's source freeze ended. Runs retain raw NumPy arrays, per-seed JSON metrics, array hashes, script hashes, runtime versions and labelled PNG maps. `run-02` is the final three-seed experiment; `run-01` is an earlier draft with unweighted catchment moisture and a basin statistic that included ocean depth. Its original source is archived at `run-01/source.py`. Do not use that earlier basin statistic.

The domain is a synthetic 32 km square in global metre coordinates. Three seeds—82317, 1447, 9001—vary a long curved fault constraint and smaller relief phases. This is an illustrative island with ocean boundary conditions, **not** a proposed relocation of Sanctuary's eleven destinations.

The stages are:

1. An analytic curved uplift band and oblique secondary band define the large relief. Small globally addressed waves seed tributary variation. A finite coastal margin supplies outlets. The uplift field is an authored magnitude constraint; it is not a plate collision simulation.
2. Six drainage/implicit stream-power passes modify relief. Area-dependent incision competes with a bounded uplift increment. Coefficients express design iteration, without a geological time calibration or sediment mass conservation.
3. Priority-flood returns spill levels and a directed acyclic receiver graph. Catchment area accumulates upstream. The original bed remains below retained lake levels; the graph may cross uphill **bed** edges within those lakes, but never climbs the spill surface.
4. A west-to-east moist column loses water through rainout, with explicit sea replenishment, cloud conversion, delayed fallout and lee evaporation. Reversing wind is an intervention on the same terrain. Moisture values are dimensionless, fixed-scale indices rather than rainfall in mm/year.
5. Rain-weighted upstream flow, local rain, slope and a prescribed thermal gradient produce moisture and soil-retention proxies. Six continuous suitability scores blend woodland, meadow, riparian vegetation, alpine habitat, dry scrub and warm wet forest. These are habitat affinities, not a new taxonomy for the eleven saved Sanctuary region IDs. Temperature lapse and the across-map thermal gradient are design inputs, not validated regional climatology.
6. A shortest-cost route search rejects coarse edges above grade 0.55. It selects a pass without changing the terrain. It does not establish sub-grid path width, riding clearance, a walkable cross-slope or camera clearance.

### Measurements

Final three-seed run: **3.66 seconds**, NumPy 2.3.5, bundled Python on arm64. The initial run, final run, separate-process replay and 256² refinement total **11.83 seconds** of measured experiment wall time. No Wrela build or GPU was used. A 110-second guard bounds each experiment command.

| Seed / grid | Highest point (m) | Land grade p95 / max | Largest catchment (km²) | Maximum land basin depth (m) | Route max edge grade |
| --- | ---: | ---: | ---: | ---: | ---: |
| 82317 / 192² | 1407.83 | 0.396 / 0.742 | 180.26 | 37.11 | 0.331 |
| 1447 / 192² | 1421.46 | 0.393 / 0.992 | 161.26 | 14.97 | 0.290 |
| 9001 / 192² | 1438.29 | 0.386 / 0.964 | 132.35 | 24.79 | 0.257 |

All final seeds route 100% of land cells to sea outlets. All receiver graphs are acyclic and have zero uphill spill edges. Catchment area is conserved to at most 2.39e-7 m² absolute error over the roughly billion-square-metre sampled domain. The raster uses node-associated `dx²` areas, including boundary nodes, so its quadrature area slightly exceeds the exact 32 km square; this is a graph conservation test, not a precise coastline area estimate.

West-wind mean moisture indices on matched bands around the main ridge are **0.421 / 0.114**, **0.496 / 0.099**, and **0.425 / 0.093** for west/east sides. Reversing wind changes these to **0.062 / 0.371**, **0.065 / 0.381**, and **0.082 / 0.405**. Column reservoir residuals stay below 1.8e-15. Seed 82317's continuous suitability changes by RMS **0.198** under wind reversal. This is causal sensitivity in the prototype, not meteorological validation.

Repeating seed 82317 in a separate Python process produced identical arrays, maximum error zero and array SHA-256 `f016bdd247d43ce9eee22c514f158f1cd0cb47272e8d5b9381a0eba0813ccccd`. The final experiment script SHA-256 is `b832da4dbdc3fc337253740c7c7454e2af25d45080acc01272f35b096aea14f9`. Reproducibility is established for this recorded environment, not yet Swift, other NumPy versions or different floating-point architectures.

At 256², seed 82317 retains the broad ridge, rain shadow and main catchments. Its largest catchment changes from 180.26 to 178.77 km² and basin depth from 37.11 to 41.56 m. Angular tributaries persist. **Resolution stability is partial; this is not a convergence certificate.**

### Inspected research maps

All three first-run boards and all three final boards were inspected, together with the refinement board and radial comparison. The final rain-weighted-flow correction reduces some dry-side riparian striping; it does not remove the grid-shaped channels or establish ecological realism.

- [Final causal board, seed 82317](../../.build/sanctuary-native-20260912/constraint-biome-experiment/run-02/seed-82317-research.png)
- [256² refinement of the same seed](../../.build/sanctuary-native-20260912/constraint-biome-experiment/refinement-256/seed-82317-research.png)
- [Current radial formula and wind-reversal comparison](../../.build/sanctuary-native-20260912/constraint-biome-experiment/comparison-research.png)
- [Raw final manifest](../../.build/sanctuary-native-20260912/constraint-biome-experiment/run-02/manifest.json), [replay/refinement comparison](../../.build/sanctuary-native-20260912/constraint-biome-experiment/comparison.json)

The elongated divide and feeder valleys are coherent at a broad scale. The square coastal margin and long straight D8 channel runs are artificial. Terrain incision creates comb-like slopes; soil inherits drainage stripes. The high-point color scale saturates above 1015 m, making the upper ridge white; these pixels are not simulated snow. Dry scrub wins 65.6–68.6% of land and warm wet forest only 4.9–6.5%, which is a poor first fit for a welcoming, varied Sanctuary. Soil is neither pedogenesis nor sediment transport; no infiltration or water-table solver was implemented.

## Comparison with current production ownership

`BiomeGeography.sample` computes normalized Gaussian weights from distances to named landmarks. `Terrain.globalHeight` blends biome elevation/relief, adds regional analytic ridges/summits, and applies route and waterbed constraints. `BiomeHabitatField` then combines those weights with slope, route exclusion and creek proximity. These sources give stable, cheap coordinate queries and deliberate destinations, but rainfall and upstream drainage do not cause their biome distribution.

`compare.py` parses the current eleven landmark/radius constants and translates the exact radial-weight formula. It does **not** run Swift or claim production numerical equivalence. The left comparison map has no wind input; its wind response is zero by definition. The experiment's right maps move habitat affinity when wind reverses without moving mountains. Different categories and palettes mean the colors are not a direct quality score. Production file fingerprints at read time are in `comparison.json`.

Keep the architecture from `ARCHITECTURE.md` and `TESTING.md`: FieldCore owns reusable metre-valued mathematics and bounds; SanctuaryContent owns seed, generation constraints, drainage/ecology semantics and versioned world state. Project code supplies meshes/materials. The global grid, drainage graph, textures and meshes are compiled representations of eventual Swift procedural source and its constraints, never hand-painted truth or a second renderer.

## Decision and production gates

Proceed next with a **source-contract and graph-quality study**, not a terrain replacement. Retain uplift/divide, outlet, climate and habitat causality; investigate irregular drainage graphs or continuous channel reconstruction before increasing detail. Merely smoothing the displayed raster would hide the grid without repairing channel connectivity or collision agreement.

The eventual source should expose bed elevation/gradient, drainage segment and basin IDs, spill level, water coverage/depth, long-term moisture/temperature/soil proxies and continuous habitat affinities at a world coordinate. Keep prevailing climate separate from live weather: a rainstorm changes wetness and local conditions without respawning whole forests or regenerating mountains. Root stream identity should remain stable when render chunks enter or leave memory.

Routes and elevation constraints must participate before final generation acceptance. Preserve authored summit ranges and pass targets; test longitudinal and full-vector grade, path width and ordinary-eye view clearance at production resolution. This study's 167.54/125.49 m samples cannot justify a one-metre riding or shoreline claim. A failed route should reject/regenerate the candidate or request an authored pass constraint, not blend a tall mountain into a nearby low target across a narrow verge. The previous route correction lesson remains applicable.

Water requires an explicit distinction between ordinary pits to breach, designed lakes with spill outlets, and intentional closed basins. Do not silently pour every generated basin to its spill level. Couple potential depth to supply/storage policy; preserve graph connectivity under accepted local edits. Resolve basin water and stream surfaces through the existing production water-field contract, with near-player bed/coverage/surface consistency and no duplicated decorative water geometry. This experiment guarantees paths to the specified coast only, not credible lake volumes or river widths.

The analytic uplift source has zero tile-query error because it uses global coordinates and a seed-global constraint set. Hydrology is nonlocal: independently generating a 512 m chunk with a small halo cannot know all upstream area or downstream spill saddles. Compile a bounded whole-world drainage graph, or use stable basin partitions with explicit boundary flux/spill contracts. Cache that representation outside the frame loop; local mesh patches sample the same source and exact shared boundaries. The measured Python times establish a research budget only, not Swift stream-update or 60 Hz performance.

Preserve existing worlds by recording a generator version, seed, constraint recipe/hash and stable semantic IDs. Keep the current generator available for old saves; offer a separately created experimental world before considering migration. Existing cabin terrain, landmark IDs/coordinates, boulder edits, buildings, vegetation records and player progress must not move silently. For a future new world, use landmarks as protected constraints or select them from suitable generated sites before IDs are persisted. Layer ordered user edits above an immutable versioned base; explicitly version and invalidate affected drainage dependencies when an edit changes a watershed. Save recipes and edits, not disposable raster caches. A macro regenerate is not an ordinary local garden edit.

Before production adoption: deterministic Swift source/query tests; outlet, lake, seam and grade invariants; save round-trip and old-version preservation; mesh/query agreement near players; isolated Soundstage landform/water studies; then native eleven-destination travel, ordinary ground-level views and short measured streaming workloads. This research completes none of those production acceptance gates.

## Replay

### Final topology follow-up: true Delaunay

The final bounded follow-up replaced the D8/lattice topology with globally seeded uniform interior sites and true SciPy/Qhull Delaunay connectivity, keeping seed 82317, fixed boundary outlets and the same uplift/drainage assumptions. [Full finding](../../.build/sanctuary-native-20260912/constraint-biome-graph-study/delaunay/STUDY.md), [actual geometry-only research map](../../.build/sanctuary-native-20260912/constraint-biome-graph-study/delaunay/run-01/graph-comparison-research.png), and [raw metrics/source hashes](../../.build/sanctuary-native-20260912/constraint-biome-graph-study/delaunay/run-01/metrics.json) are retained under `.build/sanctuary-native-20260912/constraint-biome-graph-study/delaunay/`.

SciPy 1.18.1 was installed from official PyPI into that study's isolated `dependencies/` directory in 3.274 seconds; package provenance, wheel hash, source and license notices are recorded in the linked report. Experiment plus exact replay took 6.848 seconds. No production or Tools source changed for this follow-up.

The inspected map and directional measures support the narrow topology finding: major-channel eighth directional moment dropped from D8's 1.000 to 0.0165; fourth moment dropped from 0.196 to 0.0820. Channel length within 5° of compass axes fell from 100% to 22.27%, close to the 22.22% expectation for uniformly distributed directions. All land reaches sea, graphs remain acyclic, uphill spill edges remain zero, and every replay array matches exactly. Potential lake beds and spill surfaces remain distinct.

This supersedes the earlier jittered-cell control's limited improvement; it does **not** remove the production hold. Uniform sites leave edges from 0.799 to 663.008 m, an unsuitable uncontrolled sampling range for final terrain. Lake occupancy/supply, continuous banks, source/mesh collision agreement, protected routes/landmarks and versioned save behavior remain unresolved. The result supports an irregular global drainage source in a future production design, not copying the experimental graph or its noisy basin dots into the game. No further topology execution is planned in this tranche.

### Original causal-study replay

The final scripts are retained under `Tools/Experiments/sanctuary_biomes/`; measured artifacts remain in the staging root above. Their hashes match the recorded experiment. No production source was edited for this research.

Use the bundled Python (NumPy and Pillow; no installs) and fresh output directories:

```sh
PYTHON=/Users/ryanwible/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/bin/python3
EXPERIMENT=Tools/Experiments/sanctuary_biomes
$PYTHON "$EXPERIMENT/experiment.py" --output /tmp/sanctuary-biomes-replay/run-02
$PYTHON "$EXPERIMENT/experiment.py" --output /tmp/sanctuary-biomes-replay/replay-82317 --seeds 82317
$PYTHON "$EXPERIMENT/experiment.py" --output /tmp/sanctuary-biomes-replay/refinement-256 --seeds 82317 --grid 256
$PYTHON "$EXPERIMENT/compare.py" --root /tmp/sanctuary-biomes-replay --repo .
```

The script rejects more than three seeds, grids above 256², more than eight erosion passes and reused output directories. The research remains limited to three distinct seeds.
