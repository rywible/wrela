# Sanctuary biome density research — September 12, 2026

Status: bounded source research and one CPU distribution experiment. No production
source, Swift build, renderer, GPU, collision integration, or native image was exercised.
The candidate density values are not visually accepted.

## Source problem

Sanctuary has a continuous 32 × 32 km biome influence field, but its current placement
contract discards most of that information. `SanctuaryGeography.plan(for:)` seeds each
512 m chunk independently, chooses a constant count from the chunk-center primary biome,
and places every feature uniformly inside an 18 m inset. Woodland and rainforest receive
52 records per chunk, creek receives 40, and feature IDs are chunk-local.

The familiar area-per-point spacing is `sqrt(512² / count)`: 71.0 m for 52 records and
81.0 m for 40. For a uniform Poisson process, expected nearest-neighbor distance is half
that value, 35.5 m and 40.5 m respectively. The experiment's conservative baseline,
which counts every source feature as vegetation even when woodland would make about 22%
of them boulders, measured 34.5–38.5 m mean nearest-neighbor distance. This agrees with
the source mathematics.

Only the active 3 × 3 chunk neighborhood receives these records. The complete-world
terrain outside it has no vegetation records, cluster silhouettes, or habitat coverage
LOD. More detailed tree meshes cannot solve that distribution and residency gap.

## Primary research

Lane and Prusinkiewicz distinguish individual local-to-global ecosystem simulation from
inverse global-to-local placement inferred from plant-density fields. Their multilevel
plant-community model explicitly treats clustering and succession as distribution
phenomena and substitutes detailed plants for coarse distribution representations. That
supports a content-owned density field and parent/child records rather than renderer-owned
random placement: [Generating Spatial Distributions for Multilevel Models of Plant Communities](https://algorithmicbotany.org/papers/eco.gi2002.html).

Deussen et al. combine terrain and environmental interactions with plant distributions,
then reduce large scene complexity by instancing representative plants, groups, or plant
organs. This supports keeping distribution identity independent of a particular mesh and
using a cluster record as a presentation summary, not a second simulation:
[Realistic Modeling and Rendering of Plant Ecosystems](https://graphics.stanford.edu/papers/ecosys/).

Bridson's Poisson-disk construction supplies a simple minimum-distance point process with
linear expected generation time in the number of produced samples. Sanctuary needs the
minimum-distance property to prevent overlapping trunks, but it also needs a world-space
candidate identity and query halo so chunk order cannot change acceptance:
[Fast Poisson Disk Sampling in Arbitrary Dimensions](https://www.cs.ubc.ca/~rbridson/docs/bridson-siggraph07-poissondisk.pdf).

Kopf et al. demonstrate deterministic tile-based blue-noise points over arbitrarily large
areas, local regeneration, spatially varying density, and recursive detail. Their result is
a stronger future option if hashed candidate-lattice cost remains excessive:
[Recursive Wang Tiles for Real-Time Blue Noise](https://johanneskopf.de/publications/blue_noise/paper/Recursive_Wang_Tiles_For_Real-Time_Blue_Noise.pdf).

Losasso and Hoppe's geometry clipmaps render the finest level as a filled region and
coarser levels as hollow rings, with shared grid boundaries and transition morphing. This
study does not propose a new terrain renderer. It applies the same residency invariant to
vegetation records: a coarser parent must omit the region represented by finer children,
and ring transitions must never draw both:
[Geometry Clipmaps](https://hhoppe.com/geomclipmap.pdf).

## Experiment

The standalone prototype is in
`.build/sanctuary-native-20260912/biome-density-experiment`. It uses seed 82317 and
Python 3.14.6 on `macOS-26.6.2-arm64-arm-64bit-Mach-O`.

One 3 × 3 chunk region was sampled around each named biome. The proposed source uses:

- existing normalized Gaussian biome weights as continuous habitat input;
- a continuous distance-to-creek corridor for riparian affinity;
- stable 256 m world-cell cluster parents with 45–115 m influence radii;
- one jittered candidate per global microcell;
- deterministic probability acceptance from habitat × cluster influence;
- deterministic priority thinning against a minimum-distance halo; and
- stable individual IDs `veg:<profile>:<global-x>:<global-z>`.

The baseline uses 52 or 40 uniformly distributed records per chunk. It intentionally gives
the current source credit for every record as vegetation, so it overstates current woodland
canopy and creek tree density.

| Profile | Method | Records/chunk | NN mean / p10 / p90 | Occupied 32 m cells | 5 m canopy coverage proxy | 64 m count Fano |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Woodland | Current uniform | 52.0 | 36.1 / 13.1 / 61.9 m | 18.6% | 28.8% | 0.89 |
| Woodland | Hierarchical field | 376.2 | 15.0 / 9.3 / 22.5 m | 73.2% | 85.3% | 2.31 |
| Rainforest | Current uniform | 52.0 | 34.5 / 11.5 / 58.4 m | 18.1% | 30.5% | 0.94 |
| Rainforest | Hierarchical field | 810.8 | 10.8 / 7.3 / 15.4 m | 90.4% | 96.2% | 3.84 |
| Creek | Current uniform | 40.0 | 38.5 / 14.6 / 70.8 m | 14.3% | 23.0% | 1.05 |
| Creek | Hierarchical field | 59.7 | 11.7 / 6.3 / 18.4 m | 8.9% | 10.6% | 10.86 |

A Fano factor near one is consistent with uniform randomness. Values above one show the
intended patchiness. Creek covers less total area than the uniform baseline because records
are concentrated along the habitat corridor rather than scattered across the chunk; its
high Fano factor captures that narrow ecotope.

Independent queries of four adjacent chunks produced exactly the same ID-position tuples
as one combined 2 × 2 query for all profiles: 0 mismatches across 1,526 woodland, 3,360
rainforest, and 293 creek records. Grouping the same children into stable 128 m parents
preserved child counts exactly: 3,386 woodland children in 144 occupied parents, 7,297
rainforest children in 144, and 537 creek children in 26.

## Query cost and bound

| Profile | Python median / p95 / max per 512 m query | Worst unique candidates | Worst priority comparisons | Worst macro-field evaluations |
| --- | ---: | ---: | ---: | ---: |
| Woodland | 96.0 / 98.3 / 98.6 ms | 2,359 | 17,511 | 21,231 |
| Rainforest | 137.2 / 142.6 / 154.6 ms | 3,349 | 34,438 | 30,141 |
| Creek | 182.5 / 185.9 / 186.7 ms | 4,399 | 6,149 | 39,591 |

These are deliberately reported rather than projected to Swift. The direct candidate
lattice costs time according to queried area, even when creek habitat rejects nearly every
candidate. It is unsuitable for synchronous rebuilding at a chunk boundary in this form.
The fixed cell size, fixed halo, nine macro influences, and bounded neighbor stencil do,
however, give a conservative operation bound per query.

Production integration should generate an entering ring ahead of the player, cache records
by global source key, and commit completed chunks atomically. A density or garden revision
should invalidate only intersecting parents. If measured cached Swift generation still
misses the update budget, replace the microcell enumerator with progressive recursive Wang
tiles so cost follows accepted density more closely; do not reduce ordinary live updates or
hide the hitch in a one-time load measurement.

## Proposed game-owned API

The habitat field remains source data in `SanctuaryContent`:

```swift
public struct SanctuaryHabitatSample: Equatable, Sendable {
  public let coordinate: SIMD2<Float>
  public let biomeWeights: [SanctuaryBiome: Float]
  public let canopyPotential: Float
  public let understoryPotential: Float
  public let riparianPotential: Float
  public let groundcoverPotential: Float
  public let terrainSuitability: Float
  public let routeClearance: Float
}

public struct SanctuaryHabitatField: Sendable {
  public func sample(at coordinate: SIMD2<Float>) throws -> SanctuaryHabitatSample
}
```

`biomeWeights` reuses `SanctuaryGeography.sample`. Terrain suitability derives from the
same terrain normal/height source; riparian potential derives from the shared creek/water
field; route clearance derives from `distanceToRoute`. Each scalar must be continuous and
bounded 0...1 except route clearance, which remains metres. Presentation does not modify
these values.

World-space distribution records are independent of the requesting chunk:

```swift
public struct SanctuaryEcotopeKey: Hashable, Codable, Sendable {
  public let level: Int       // 0 individual, 1 = 128 m parent, higher levels double
  public let x: Int
  public let z: Int
}

public struct SanctuaryVegetationRecord: Equatable, Sendable {
  public let id: String       // schema + layer + level-0 global cell, never chunk-local
  public let parent: SanctuaryEcotopeKey
  public let coordinate: SIMD2<Float>
  public let kind: SanctuaryFeatureKind
  public let scale: Float
  public let yaw: Float
  public let collisionRadius: Float
  public let collisionHeight: Float
  public let lodImportance: Float
}

public struct SanctuaryVegetationClusterRecord: Equatable, Sendable {
  public let id: String
  public let key: SanctuaryEcotopeKey
  public let bounds: SanctuaryMapBounds
  public let childCount: Int
  public let centroid: SIMD2<Float>
  public let canopyCoverage: Float
  public let maximumHeight: Float
  public let representativeMix: [SanctuaryFeatureKind: Float]
  public let childDigest: UInt64
}

public struct SanctuaryVegetationQuery: Sendable {
  public let exact: [SanctuaryVegetationRecord]
  public let clusters: [SanctuaryVegetationClusterRecord]
}

public struct SanctuaryVegetationSource: Sendable {
  public func query(in bounds: SanctuaryMapBounds, exactThroughLevel: Int)
    -> SanctuaryVegetationQuery
  public func feature(id: String) -> SanctuaryVegetationRecord?
}
```

The exact record is the single transform/provenance source for both visual instances and
trunk collision. Cluster records are presentation summaries of those children and never
create collision. The near query returns exact children; each farther hollow ring returns
only the coarsest requested parents. `childDigest` detects a stale parent cache. Stable IDs
allow boulder-like persisted edits later without tying saves to a chunk's current population.
Existing landmarks and current persisted feature IDs remain separate and unchanged.

A first integration should use exact records only inside the current active neighborhood,
128 m parents through roughly 4 km, and larger habitat tiles beyond. Those distances and
the experiment's high candidate counts require native occlusion, silhouette, frame-time,
and collision-grid measurement before acceptance. The experiment establishes distribution
statistics and deterministic seams; it does not establish that 376 woodland or 811 rainforest
records per chunk look good, fit memory, or sustain 60 Hz.

## Decision

Adopt continuous habitat fields, globally keyed individual records, and parent summaries as
the content contract. Keep chunk keys as query/cache windows rather than identity. The first
Swift implementation uses a constrained-jitter global lattice because it provides a separation
floor without neighbor searches and its seams are testable. Prepare/cache entering records off
the movement path and render vegetation in nonoverlapping detail rings. Retain priority-thinned
Poisson candidates or recursive Wang tiles as measured options if native review shows the
lattice structure or source generation cost is unacceptable.

The next integration gate is CPU-only: exact cross-chunk IDs, parent child-count/digest
agreement, bounded cache invalidation, and collision transforms equal to visual source
transforms. Native review must then judge canopy mass, ecotope edges, horizon continuity,
popping, and live update hitches. Nothing in this study demonstrates those visual results.

## Production foundation added after the experiment

The first source-only implementation now lives in `BiomeHabitatField.swift` and
`BiomeVegetationPopulation.swift`. It deliberately uses a more conservative policy than the
experiment:

- one globally keyed candidate per 32 m cell, rather than 8–12 m cells;
- jitter constrained to 20–80% of a cell, giving candidates in axis-adjacent cells at least
  12.8 m separation without a neighbor search;
- a hard 0.34 acceptance ceiling, or at most 87 expected accepted candidates per 512 m chunk
  before habitat, water, route, slope, and cabin rejection;
- stable 128 m parents containing exactly 16 candidate cells; and
- at most 4,096 complete candidate cells in one query.

The 4,096-cell cap allows sixteen 512 m chunks only when query edges are parent-aligned; parent
rounding can reduce the accepted request size. Oversized queries reject before enumeration.
This is intended for the current 3 × 3 exact neighborhood plus incremental prefetch, not a
whole-world exact query.

One 512 m chunk covers 256 candidate cells and evaluates at most 2,304 Gaussian cluster-parent
terms. A 3 × 3 exact neighborhood covers 2,304 candidates and 20,736 cluster terms; the hard
query cap covers 4,096 candidates and 36,864 terms. Habitat sampling also evaluates the real
terrain and water sources, so these operation counts are an expectation for root's CPU timing,
not a frame-time prediction.

The acceptance ceiling is the explicit initial density tuning knob. This conservative foundation
is intentionally far below the experiment's 376 woodland and 811 rainforest records per chunk.
It establishes deterministic source ownership and LOD summaries; it does not claim a dense forest
or resolve the empty-horizon result without later native density and residency integration.

The habitat source exposes continuous biome, riparian, terrain-slope, and route suitability.
Exact `Terrain.water(at:)` and the existing `SanctuaryTerrainTessellation.cabinExtent` are
separate eligibility exclusions. Placement IDs are `vegetation-v1:<global-cell-x>:<global-cell-z>`;
request bounds never enter an ID or random seed. Queries use half-open maximum edges except at
the finite world's maximum, so adjacent cache windows do not duplicate records.

Each returned 128 m parent summary is calculated from its complete children even when a request
intersects only part of that parent. Its child digest includes IDs, species, and scale seeds.
The summary explicitly reports no collision authority. Future presentation and collision code
must derive near transforms from the exact record; a far parent may summarize those children for
presentation but cannot create a collision solid.

The implementation has source tests for deterministic replay, split and overlapping bounds,
complete parent summaries, duplicate rejection, finite transforms, real water/cabin exclusions,
and pre-enumeration query limits. Root owns compilation and test execution while native renderer
measurement is active. The expected-count ceiling is an algorithmic bound, not a measured
population count or frame-time result.
