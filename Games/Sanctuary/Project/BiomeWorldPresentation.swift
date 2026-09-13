import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import simd

struct SanctuaryStreamUpdate {
  let keys: Set<SanctuaryTerrainChunkKey>
  let removedBatchNames: Set<String>
  var batches: [SceneBatch]
  var generation: UInt64 = 0
}

struct BiomePresentationTemplates {
  let trunk: Mesh
  let broadCrown: Mesh
  let conifer: [SceneBatch]
  let cactus: Mesh
  let stone: Mesh
  let driftwood: Mesh
  let willow: [SceneBatch]
  let palm: [SceneBatch]
  let vegetationFull: [SanctuaryVegetationSpecies: [SceneBatch]]
  let vegetationMiddle: [SanctuaryVegetationSpecies: [SceneBatch]]
  let vegetationFar: [SanctuaryVegetationSpecies: [SceneBatch]]

  init(trunk: Mesh, broadCrown: Mesh, stone: Mesh) throws {
    self.trunk = trunk
    self.broadCrown = broadCrown
    self.stone = stone
    // Both full source meshes and reduced representations are created once, not per chunk.
    willow = BiomeVegetationDesign.make(.willow).batches(name: "Regional willow")
    palm = BiomeVegetationDesign.make(.palm).batches(name: "Regional palm")
    conifer = BiomeConiferDesign.defaultBatches
    cactus = try Mesher.compile(SanctuaryLayout.cactusField, resolution: 18)
    driftwood = try Mesher.compile(SanctuaryLayout.driftwoodField, resolution: 18)
    // Preserve every seeded leaf boundary in the distance representation. Only the
    // 35 mm interior ridge is removed; the existing selector bounds its projected error.
    let broadLeaves = Mesher.leaves(on: broadCrown, count: 1800)
    let broadLeavesFlat = Mesher.leaves(on: broadCrown, count: 1800, flattened: true)
    let broad = [
      SceneBatch(name: "Population broadleaf wood", mesh: trunk, instances: [Instance(tint: V3(0.38, 0.27, 0.18), kind: 4)],
        lodMeshes: [MeshProcessing.simplify(trunk, ratio: 0.18, error: 0.035)], roughness: 0.9),
      SceneBatch(name: "Population broadleaf crown", mesh: broadCrown, instances: [Instance(tint: V3(0.37, 0.55, 0.27), kind: 2)],
        lodMeshes: [MeshProcessing.simplify(broadCrown, ratio: 0.12, error: 0.08)], roughness: 0.88),
      SceneBatch(name: "Population broadleaf leaves", mesh: broadLeaves,
        instances: [Instance(tint: V3(0.37, 0.55, 0.27), kind: 8)],
        lodMeshes: [broadLeavesFlat], roughness: 0.86, doubleSided: true),
    ]
    let far = BiomeWorldPresentation.vegetationCanopyTemplates(broadCrown: broadCrown)
    func prepared(_ source: [SanctuaryVegetationSpecies: [SceneBatch]])
      -> [SanctuaryVegetationSpecies: [SceneBatch]] {
      source.mapValues { batches in
        batches.map { source in
          var batch = source
          batch.prepareUpload()
          return batch
        }
      }
    }
    vegetationFar = prepared(far)
    vegetationFull = prepared([.broadleaf: broad, .willow: willow, .palm: palm, .conifer: conifer,
      .cactus: [SceneBatch(name: "Population cactus", mesh: cactus,
        instances: [Instance(tint: V3(0.31, 0.48, 0.29), kind: 3)], roughness: 0.82)],
      .reeds: far[.reeds] ?? []])
    var middle: [SanctuaryVegetationSpecies: [SceneBatch]] = [:]
    for species in SanctuaryVegetationSpecies.allCases {
      var representations = far[species] ?? []
      if [.broadleaf, .willow, .palm, .conifer].contains(species),
        let wood = vegetationFull[species]?.first {
        var reduced = wood
        reduced.mesh = wood.lodMeshes.last ?? wood.mesh
        reduced.lodMeshes = []
        representations.insert(reduced, at: 0)
      }
      middle[species] = representations
    }
    // Payloads stay with the bounded immutable species/tier templates. Copying them
    // for residency changes only instances; changing a mesh invalidates its metadata.
    vegetationMiddle = prepared(middle)
  }
}

enum BiomeWorldPresentation {
  /// Small original far representations. Every instance keeps its exact record transform;
  /// these canopy silhouettes never replace a community with one oversized forest blob.
  static func vegetationCanopyTemplates(broadCrown: Mesh) -> [SanctuaryVegetationSpecies: [SceneBatch]] {
    func batch(_ species: SanctuaryVegetationSpecies, _ mesh: Mesh, _ color: V3) -> [SceneBatch] {
      [SceneBatch(name: "Population \(species.rawValue) distant canopy", mesh: mesh,
        instances: [Instance(tint: color, kind: 8)], roughness: 0.88, doubleSided: true)]
    }
    func oval(_ center: V3, _ radius: V3) -> Mesh {
      ParametricMesh.ellipsoid(center, radius, detail: 3)
    }
    var lo = V3(repeating: Float.greatestFiniteMagnitude), hi = -lo
    for vertex in broadCrown.vertices {
      let p = V3(vertex.position.x, vertex.position.y, vertex.position.z)
      lo = simd_min(lo, p); hi = simd_max(hi, p)
    }
    let center = (lo + hi) * 0.5, size = hi - lo
    let broad = ParametricMesh.joined([
      oval(center, size * V3(0.34, 0.44, 0.36)),
      oval(center + size * V3(-0.25, -0.08, 0.02), size * V3(0.24, 0.32, 0.29)),
      oval(center + size * V3(0.25, -0.11, 0.04), size * V3(0.24, 0.30, 0.29)),
      oval(center + size * V3(0.01, -0.12, -0.26), size * V3(0.28, 0.32, 0.23)),
    ])
    let willow = ParametricMesh.joined((0..<6).map { index in
      let angle = Float(index) * 2.399
      return oval(V3(cos(angle) * 1.55, 2.85 + 0.18 * sin(angle * 2), sin(angle) * 1.5),
        V3(0.92, 1.65 + 0.12 * cos(angle), 0.88))
    })
    var fir = Mesh()
    for level in 0..<5 {
      let base: Float = 1.45 + Float(level) * 1.22
      let radius: Float = 2.4 - Float(level) * 0.43
      let tip = V3(0.05, min(8, base + 2.45), 0.06)
      for side in 0..<8 {
        let a = Float(side) * .pi / 4, b = Float(side + 1) * .pi / 4
        let p = V3(cos(a) * radius, base, sin(a) * radius)
        let q = V3(cos(b) * radius, base, sin(b) * radius)
        let normal = normalize(cross(tip - p, q - p))
        fir.triangle(Vertex(p, normal, V3(repeating: 1)), Vertex(tip, normal, V3(repeating: 1)), Vertex(q, normal, V3(repeating: 1)))
        fir.triangle(Vertex(V3(0, base, 0), V3(0, -1, 0), V3(repeating: 1)),
          Vertex(p, V3(0, -1, 0), V3(repeating: 1)), Vertex(q, V3(0, -1, 0), V3(repeating: 1)))
      }
    }
    var palm = Mesh()
    let palmHub = SanctuaryBotanicalTrunk(.palm).point(at: 1)
    for frond in 0..<8 {
      let angle = Float(frond) * .pi / 4
      let direction = V3(cos(angle), 0, sin(angle)), side = V3(-sin(angle), 0, cos(angle))
      func p(_ t: Float, _ sign: Float) -> V3 {
        palmHub + direction * (3.05 * t) + V3(0, 0.75 * sin(t * .pi) - 1.12 * t * t, 0)
          + side * (sign * 0.30 * sin(t * .pi))
      }
      for segment in 0..<3 {
        let a = Float(segment) / 3, b = Float(segment + 1) / 3
        let x = p(a, -1), y = p(a, 1), z = p(b, -1), w = p(b, 1)
        if segment > 0 { palm.triangle(Vertex(x, V3(0, 1, 0), V3(repeating: 1)), Vertex(z, V3(0, 1, 0), V3(repeating: 1)), Vertex(y, V3(0, 1, 0), V3(repeating: 1))) }
        if segment < 2 { palm.triangle(Vertex(y, V3(0, 1, 0), V3(repeating: 1)), Vertex(z, V3(0, 1, 0), V3(repeating: 1)), Vertex(w, V3(0, 1, 0), V3(repeating: 1))) }
      }
    }
    let cactus = ParametricMesh.tube(segments: 3, sides: 6,
      center: { V3(0, $0 * 3.4, 0) }, radius: { _ in 0.34 })
    var reeds = Mesh()
    for stem in 0..<7 {
      let angle = Float(stem) * 2.399, radius = Float(stem % 3) * 0.16
      let base = V3(cos(angle) * radius, 0, sin(angle) * radius)
      let tip = base + V3(cos(angle) * 0.16, 1.15 + Float(stem % 4) * 0.12, sin(angle) * 0.16)
      let side = V3(-sin(angle), 0, cos(angle)) * 0.055
      reeds.triangle(Vertex(base - side, V3(0, 1, 0), V3(repeating: 1)),
        Vertex(tip, V3(0, 1, 0), V3(repeating: 1)), Vertex(base + side, V3(0, 1, 0), V3(repeating: 1)))
    }
    return [.broadleaf: batch(.broadleaf, broad, V3(0.37, 0.55, 0.27)),
      .willow: batch(.willow, willow, V3(0.38, 0.52, 0.27)),
      .palm: batch(.palm, palm, V3(0.28, 0.47, 0.29)),
      .conifer: batch(.conifer, fir, V3(0.25, 0.39, 0.29)),
      .cactus: batch(.cactus, cactus, V3(0.31, 0.48, 0.29)),
      .reeds: batch(.reeds, reeds, V3(0.42, 0.51, 0.22))]
  }

  static func vegetationBatches(
    placements: [SanctuaryVegetationResidencyTier: [SanctuaryVegetationPlacement]],
    templates: BiomePresentationTemplates
  ) -> [SceneBatch] {
    var result: [SceneBatch] = []
    for tier in SanctuaryVegetationResidencyTier.allCases {
      let source = tier == .detailed ? templates.vegetationFull
        : tier == .community ? templates.vegetationMiddle : templates.vegetationFar
      for species in SanctuaryVegetationSpecies.allCases {
        let records = (placements[tier] ?? []).filter { $0.record.species == species }
        guard !records.isEmpty else { continue }
        for template in source[species] ?? [] {
          var batch = template
          batch.name = "Vegetation tier\(tier.rawValue) \(template.name)"
          batch.instances = records.map { placement in
            var instance = template.instances[0]
            instance.model = transform(placement.position, V3(repeating: placement.record.scale), placement.record.yaw) * instance.model
            return instance
          }
          result.append(batch)
        }
      }
    }
    return result
  }
  static func groundColor(_ biome: SanctuaryBiome) -> V3 {
    BiomeGroundAppearance.biomeAlbedo(biome)
  }

  static func waterColor(_ body: SanctuaryWaterBody) -> V3 {
    switch body {
    case .creek: return V3(0.31, 0.56, 0.61)
    case .wetland: return V3(0.29, 0.47, 0.42)
    case .lake: return V3(0.25, 0.48, 0.63)
    case .tidepool: return V3(0.27, 0.58, 0.61)
    case .ocean: return V3(0.19, 0.42, 0.58)
    }
  }

  static func composedHeight(
    _ terrain: Terrain, garden: HabitatGarden?, x: Float, z: Float
  ) -> Float {
    let base = terrain.height(x, z)
    guard let garden else { return base }
    return garden.surfaceHeight(baseHeight: base, at: .init(x: x, z: z))
  }

  static func terrainMesh(
    terrain: Terrain, key: SanctuaryTerrainChunkKey, garden: HabitatGarden?
  ) -> Mesh {
    let world = SanctuaryGeography.bounds
    let baseResolution = SanctuaryTerrainTessellation.baseResolution
    let refinement = SanctuaryTerrainTessellation.refinement
    let fineResolution = baseResolution * refinement
    let refined = SanctuaryTerrainTessellation.refinedCells(in: key, garden: garden)
    var refinementCache: [SanctuaryTerrainChunkKey: Set<SanctuaryTerrainCell>] = [key: refined]
    var mesh = Mesh()
    var vertexIndices: [SIMD2<Int>: UInt32] = [:]

    func vertex(_ gx: Int, _ gz: Int) -> UInt32 {
      let grid = SIMD2(gx, gz)
      if let existing = vertexIndices[grid] { return existing }
      let p = key.minimum + SIMD2(Float(gx), Float(gz))
        * (SanctuaryTerrainChunkKey.size / Float(fineResolution))
      let h = composedHeight(terrain, garden: garden, x: p.x, z: p.y)
      let e: Float = 0.25
      let dx = composedHeight(terrain, garden: garden, x: p.x + e, z: p.y)
        - composedHeight(terrain, garden: garden, x: p.x - e, z: p.y)
      let dz = composedHeight(terrain, garden: garden, x: p.x, z: p.y + e)
        - composedHeight(terrain, garden: garden, x: p.x, z: p.y - e)
      let normal = normalize(V3(-dx, 2 * e, -dz))
      let substrate = BiomeGroundAppearance.sample(
        at: p, height: h, normal: normal, terrain: terrain, garden: garden)
      let index = UInt32(mesh.vertices.count)
      mesh.vertices.append(Vertex(V3(p.x, h, p.y), normal, substrate.albedo))
      vertexIndices[grid] = index
      return index
    }

    func neighborIsRefined(_ x: Int, _ z: Int) -> Bool {
      var neighborKey = key
      var cellX = x, cellZ = z
      if cellX < 0 { neighborKey = .init(x: neighborKey.x - 1, z: neighborKey.z); cellX += baseResolution }
      if cellX >= baseResolution { neighborKey = .init(x: neighborKey.x + 1, z: neighborKey.z); cellX -= baseResolution }
      if cellZ < 0 { neighborKey = .init(x: neighborKey.x, z: neighborKey.z - 1); cellZ += baseResolution }
      if cellZ >= baseResolution { neighborKey = .init(x: neighborKey.x, z: neighborKey.z + 1); cellZ -= baseResolution }
      guard neighborKey.intersectsWorld else { return false }
      if refinementCache[neighborKey] == nil {
        refinementCache[neighborKey] = SanctuaryTerrainTessellation.refinedCells(
          in: neighborKey, garden: garden)
      }
      return refinementCache[neighborKey]!.contains(.init(x: cellX, z: cellZ))
    }

    for z in 0..<baseResolution {
      for x in 0..<baseResolution {
        let bounds = SanctuaryTerrainTessellation.cellBounds(
          in: key, cell: .init(x: x, z: z))
        if bounds.maximum.x <= world.minimum.x || bounds.minimum.x >= world.maximum.x
          || bounds.maximum.y <= world.minimum.y || bounds.minimum.y >= world.maximum.y
        { continue }
        let cabin = SanctuaryTerrainTessellation.cabinExtent
        if bounds.minimum.x < cabin && bounds.maximum.x > -cabin
          && bounds.minimum.y < cabin && bounds.maximum.y > -cabin
        { continue }

        let bx = x * refinement, bz = z * refinement
        if refined.contains(.init(x: x, z: z)) {
          for rz in 0..<refinement {
            for rx in 0..<refinement {
              let a = vertex(bx + rx, bz + rz)
              let b = vertex(bx + rx + 1, bz + rz)
              let c = vertex(bx + rx, bz + rz + 1)
              let d = vertex(bx + rx + 1, bz + rz + 1)
              mesh.indices += [a, c, b, b, c, d]
            }
          }
          continue
        }

        let south = neighborIsRefined(x, z - 1) ? refinement : 1
        let east = neighborIsRefined(x + 1, z) ? refinement : 1
        let north = neighborIsRefined(x, z + 1) ? refinement : 1
        let west = neighborIsRefined(x - 1, z) ? refinement : 1
        var boundary: [UInt32] = []
        for i in 0...south { boundary.append(vertex(bx + i * refinement / south, bz)) }
        if east > 0 {
          for i in 1...east { boundary.append(vertex(bx + refinement, bz + i * refinement / east)) }
        }
        if north > 0 {
          for i in 1...north {
            boundary.append(vertex(bx + refinement - i * refinement / north, bz + refinement))
          }
        }
        if west > 1 {
          for i in 1..<west {
            boundary.append(vertex(bx, bz + refinement - i * refinement / west))
          }
        }
        let center = vertex(bx + refinement / 2, bz + refinement / 2)
        for i in boundary.indices {
          mesh.indices += [center, boundary[(i + 1) % boundary.count], boundary[i]]
        }
      }
    }
    return mesh
  }

  /// The established garden uses an aligned 320 × 320 one-metre lattice.
  /// Its exact composed vertices are shared with garden collision queries; tests also bound the
  /// remaining planar interpolation error between those vertices.
  static func cabinTerrainMesh(
    terrain: Terrain, garden: HabitatGarden?,
    extent: Float = SanctuaryTerrainTessellation.cabinExtent,
    resolution: Int = SanctuaryTerrainTessellation.cabinResolution
  ) -> Mesh {
    var mesh = Mesh()
    var heights = [Float](repeating: 0, count: (resolution + 1) * (resolution + 1))
    let spacing = 2 * extent / Float(resolution)
    func index(_ x: Int, _ z: Int) -> Int { z * (resolution + 1) + x }
    for z in 0...resolution {
      for x in 0...resolution {
        let px = -extent + Float(x) * spacing
        let pz = -extent + Float(z) * spacing
        heights[index(x, z)] = composedHeight(terrain, garden: garden, x: px, z: pz)
      }
    }
    for z in 0...resolution {
      for x in 0...resolution {
        let xl = max(0, x - 1), xr = min(resolution, x + 1)
        let zl = max(0, z - 1), zr = min(resolution, z + 1)
        let dx = heights[index(xr, z)] - heights[index(xl, z)]
        let dz = heights[index(x, zr)] - heights[index(x, zl)]
        let sx = Float(xr - xl) * spacing
        let sz = Float(zr - zl) * spacing
        let normal = normalize(V3(-dx / sx, 1, -dz / sz))
        let px = -extent + Float(x) * spacing
        let pz = -extent + Float(z) * spacing
        mesh.vertices.append(
          Vertex(V3(px, heights[index(x, z)], pz), normal, V3(0.37, 0.54, 0.24)))
      }
    }
    for z in 0..<resolution {
      for x in 0..<resolution {
        let a = UInt32(index(x, z)), b = a + 1, c = a + UInt32(resolution + 1)
        mesh.indices += [a, c, b, b, c, c + 1]
      }
    }
    return mesh
  }

  /// Cached far tiles cover only the world outside streamed detail. Their boundary fans
  /// expose exactly the detail lattice, so no lower duplicate surface or depth bias is needed.
  struct DistantTerrainCache {
    private let keys: [SanctuaryTerrainChunkKey]
    private let tiles: [SanctuaryTerrainChunkKey: Mesh]
    private let waterTiles: [SanctuaryTerrainChunkKey: Mesh]

    init(terrain: Terrain) {
      let bounds = SanctuaryGeography.bounds
      let lower = SanctuaryTerrainChunkKey(containing: bounds.minimum)
      let upper = SanctuaryTerrainChunkKey(containing: bounds.maximum - SIMD2(repeating: 0.01))
      var ordered: [SanctuaryTerrainChunkKey] = []
      var source: [SanctuaryTerrainChunkKey: Mesh] = [:]
      var water: [SanctuaryTerrainChunkKey: Mesh] = [:]
      for z in lower.z...upper.z {
        for x in lower.x...upper.x {
          let key = SanctuaryTerrainChunkKey(x: x, z: z)
          ordered.append(key)
          let tile = BiomeWorldPresentation.distantTerrainTile(terrain: terrain, key: key, detail: [], garden: nil)
          source[key] = tile
          water[key] = BiomeWorldPresentation.clippedWaterMesh(terrain: terrain, garden: nil, lattice: tile, largeBodiesOnly: true)
        }
      }
      keys = ordered; tiles = source; waterTiles = water
    }

    func waterMesh(terrain: Terrain, excluding detail: Set<SanctuaryTerrainChunkKey>) -> Mesh {
      var result = Mesh()
      for key in keys where !detail.contains(key) {
        let adjacent = [SanctuaryTerrainChunkKey(x: key.x - 1, z: key.z),
          .init(x: key.x + 1, z: key.z), .init(x: key.x, z: key.z - 1),
          .init(x: key.x, z: key.z + 1)].contains { detail.contains($0) }
        let tile: Mesh
        if adjacent {
          let lattice = BiomeWorldPresentation.distantTerrainTile(terrain: terrain, key: key, detail: detail,
            garden: nil, boundaryStep: 0.25)
          tile = BiomeWorldPresentation.clippedWaterMesh(terrain: terrain, garden: nil, lattice: lattice, largeBodiesOnly: true)
        } else { tile = waterTiles[key]! }
        let offset = UInt32(result.vertices.count)
        result.vertices += tile.vertices
        result.indices += tile.indices.map { $0 + offset }
      }
      return result
    }

    func mesh(terrain: Terrain, excluding detail: Set<SanctuaryTerrainChunkKey>,
      garden: HabitatGarden?) -> Mesh
    {
      var result = Mesh()
      for key in keys where !detail.contains(key) {
        let neighbors = [SanctuaryTerrainChunkKey(x: key.x - 1, z: key.z),
          .init(x: key.x + 1, z: key.z), .init(x: key.x, z: key.z - 1),
          .init(x: key.x, z: key.z + 1)]
        let touchesDetail = neighbors.contains { detail.contains($0) }
        let hasEdit = garden?.terrainPatches.contains { patch in
          let center = SIMD2(patch.center.x, patch.center.z)
          let closest = simd_min(simd_max(center, key.minimum), key.maximum)
          return length_squared(center - closest) <= patch.radius * patch.radius
        } ?? false
        let tile = touchesDetail || hasEdit
          ? BiomeWorldPresentation.distantTerrainTile(terrain: terrain, key: key, detail: detail, garden: garden)
          : tiles[key]!
        let offset = UInt32(result.vertices.count)
        result.vertices += tile.vertices
        result.indices += tile.indices.map { $0 + offset }
      }
      return result
    }
  }

  private static func distantTerrainTile(
    terrain: Terrain, key: SanctuaryTerrainChunkKey,
    detail: Set<SanctuaryTerrainChunkKey>, garden: HabitatGarden?, boundaryStep: Float = 8
  ) -> Mesh {
    let world = SanctuaryGeography.bounds
    let minimum = simd_max(key.minimum, world.minimum)
    let maximum = simd_min(key.maximum, world.maximum)
    let cabin = SanctuaryTerrainTessellation.cabinExtent
    func axis(_ low: Float, _ high: Float, origin: Float) -> [Float] {
      var values = [low, high]
      for p in [origin + 256, -cabin, cabin] where p > low && p < high { values.append(p) }
      return Array(Set(values)).sorted()
    }
    let xs = axis(minimum.x, maximum.x, origin: key.minimum.x)
    let zs = axis(minimum.y, maximum.y, origin: key.minimum.y)
    var mesh = Mesh()
    var indices: [SIMD2<Float>: UInt32] = [:]
    var refinementCache: [SanctuaryTerrainChunkKey: Set<SanctuaryTerrainCell>] = [:]
    func vertex(_ p: SIMD2<Float>) -> UInt32 {
      if let i = indices[p] { return i }
      let h = composedHeight(terrain, garden: garden, x: p.x, z: p.y)
      let e: Float = 0.25
      let dx = composedHeight(terrain, garden: garden, x: p.x + e, z: p.y)
        - composedHeight(terrain, garden: garden, x: p.x - e, z: p.y)
      let dz = composedHeight(terrain, garden: garden, x: p.x, z: p.y + e)
        - composedHeight(terrain, garden: garden, x: p.x, z: p.y - e)
      let normal = normalize(V3(-dx, 2 * e, -dz))
      let substrate = BiomeGroundAppearance.sample(
        at: p, height: h, normal: normal, terrain: terrain, garden: garden)
      let i = UInt32(mesh.vertices.count)
      mesh.vertices.append(Vertex(V3(p.x, h, p.y), normal, substrate.albedo))
      indices[p] = i
      return i
    }
    func refined(_ owner: SanctuaryTerrainChunkKey, _ cell: SanctuaryTerrainCell) -> Bool {
      if refinementCache[owner] == nil {
        refinementCache[owner] = SanctuaryTerrainTessellation.refinedCells(in: owner, garden: garden)
      }
      return refinementCache[owner]!.contains(cell)
    }
    func edge(_ a: SIMD2<Float>, _ b: SIMD2<Float>) -> [UInt32] {
      let vertical = a.x == b.x
      let fixed = vertical ? a.x : a.y
      let low = vertical ? min(a.y, b.y) : min(a.x, b.x)
      let high = vertical ? max(a.y, b.y) : max(a.x, b.x)
      if abs(fixed) == cabin && low >= -cabin && high <= cabin {
        let count = Int(round(high - low))
        return (0..<count).map { vertex(a + (b - a) * (Float($0) / Float(count))) }
      }
      let isOuter = vertical
        ? (fixed == key.minimum.x || fixed == key.maximum.x)
        : (fixed == key.minimum.y || fixed == key.maximum.y)
      let neighbor: SanctuaryTerrainChunkKey
      if vertical { neighbor = .init(x: key.x + (fixed == key.minimum.x ? -1 : 1), z: key.z) }
      else { neighbor = .init(x: key.x, z: key.z + (fixed == key.minimum.y ? -1 : 1)) }
      guard isOuter && detail.contains(neighbor) else { return [vertex(a)] }
      let distance = length(b - a)
      let direction = (b - a) / distance
      if boundaryStep < 8 {
        var values: [UInt32] = []
        for i in 0..<Int(round(distance / 4)) {
          let start = a + direction * Float(i * 4)
          let middle = start + direction * 2
          func factor(in owner: SanctuaryTerrainChunkKey) -> Int {
            let local = (middle - owner.minimum) / 4
            let cell = SIMD2<Float>(min(127, max(0, floor(local.x))), min(127, max(0, floor(local.y))))
            return waterRefinementFactor(terrain: terrain, garden: garden,
              minimum: owner.minimum + cell * 4, step: 4)
          }
          let count = max(factor(in: key), factor(in: neighbor))
          for j in 0..<count {
            values.append(vertex(start + direction * (Float(j) * 4 / Float(count))))
          }
        }
        return values
      }
      let steps = Int(round(distance / 8))
      var values: [UInt32] = []
      for i in 0..<steps {
        let start = a + direction * Float(i * 8)
        let middle = start + direction * 4
        func cell(in owner: SanctuaryTerrainChunkKey) -> SanctuaryTerrainCell {
          let local = (middle - owner.minimum) / 8
          return .init(x: min(63, max(0, Int(floor(local.x)))),
            z: min(63, max(0, Int(floor(local.y)))))
        }
        let count = refined(key, cell(in: key)) || refined(neighbor, cell(in: neighbor)) ? 8 : Int(8 / boundaryStep)
        for j in 0..<count { values.append(vertex(start + direction * (Float(j) * 8 / Float(count)))) }
      }
      return values
    }
    for z in 0..<(zs.count - 1) {
      for x in 0..<(xs.count - 1) {
        let a = SIMD2(xs[x], zs[z]), b = SIMD2(xs[x + 1], zs[z])
        let c = SIMD2(xs[x + 1], zs[z + 1]), d = SIMD2(xs[x], zs[z + 1])
        if a.x >= -cabin && c.x <= cabin && a.y >= -cabin && c.y <= cabin { continue }
        let boundary = edge(a, b) + edge(b, c) + edge(c, d) + edge(d, a)
        if boundary.count == 4 {
          mesh.indices += [boundary[0], boundary[3], boundary[1], boundary[1], boundary[3], boundary[2]]
        } else {
          let center = vertex((a + c) * 0.5)
          for i in boundary.indices {
            mesh.indices += [center, boundary[(i + 1) % boundary.count], boundary[i]]
          }
        }
      }
    }
    return mesh
  }

  static func waterMesh(
    terrain: Terrain, key: SanctuaryTerrainChunkKey, garden: HabitatGarden?, resolution: Int = 128
  ) -> Mesh {
    let world = SanctuaryGeography.bounds
    let minimum = simd_max(key.minimum, world.minimum)
    let maximum = simd_min(key.maximum, world.maximum)
    let step = SanctuaryTerrainChunkKey.size / Float(resolution)
    let width = Int(round((maximum.x - minimum.x) / step))
    let height = Int(round((maximum.y - minimum.y) / step))
    var lattice = Mesh()
    var vertices: [SIMD2<Float>: UInt32] = [:]
    var factors: [SIMD2<Int>: Int] = [:]
    func factor(_ x: Int, _ z: Int) -> Int {
      let cell = SIMD2(x, z)
      if let cached = factors[cell] { return cached }
      let low = minimum + SIMD2(Float(x), Float(z)) * step
      let result = waterRefinementFactor(terrain: terrain, garden: garden,
        minimum: low, step: step)
      factors[cell] = result
      return result
    }
    func vertex(_ p: SIMD2<Float>) -> UInt32 {
      if let i = vertices[p] { return i }
      let i = UInt32(lattice.vertices.count)
      lattice.vertices.append(Vertex(V3(p.x, 0, p.y), V3(0, 1, 0), V3(repeating: 1)))
      vertices[p] = i
      return i
    }
    func edge(_ a: SIMD2<Float>, _ b: SIMD2<Float>, _ count: Int) -> [UInt32] {
      (0..<count).map { vertex(a + (b - a) * (Float($0) / Float(count))) }
    }
    for z in 0..<height {
      for x in 0..<width {
        let count = factor(x, z)
        let spacing = step / Float(count)
        let low = minimum + SIMD2(Float(x), Float(z)) * step
        for rz in 0..<count {
          for rx in 0..<count {
            let a = low + SIMD2(Float(rx), Float(rz)) * spacing
            let b = a + SIMD2(spacing, 0), c = a + SIMD2(repeating: spacing)
            let d = a + SIMD2(0, spacing)
            let south = rz == 0 ? max(1, factor(x, z - 1) / count) : 1
            let east = rx == count - 1 ? max(1, factor(x + 1, z) / count) : 1
            let north = rz == count - 1 ? max(1, factor(x, z + 1) / count) : 1
            let west = rx == 0 ? max(1, factor(x - 1, z) / count) : 1
            let boundary = edge(a, b, south) + edge(b, c, east)
              + edge(c, d, north) + edge(d, a, west)
            if boundary.count == 4 {
              lattice.indices += [boundary[0], boundary[3], boundary[1],
                boundary[1], boundary[3], boundary[2]]
            } else {
              let center = vertex((a + c) * 0.5)
              for i in boundary.indices {
                lattice.indices += [center, boundary[(i + 1) % boundary.count], boundary[i]]
              }
            }
          }
        }
      }
    }
    return clippedWaterMesh(terrain: terrain, garden: garden, lattice: lattice)
  }

  private static func waterRefinementFactor(
    terrain: Terrain, garden: HabitatGarden?, minimum low: SIMD2<Float>, step: Float
  ) -> Int {
    let high = low + SIMD2(repeating: step)
    let waterPatches = garden?.patches.filter { $0.planting == .shallowWater } ?? []
    var result = 1
    // Refine only the narrow shoreline band. Signed dry-side samples catch a bank even
    // when the cell center itself is dry; deep interiors keep the inexpensive 4 m grid.
    let center = (low + high) * 0.5
    let base = terrain.height(center.x, center.y)
    let bed = garden?.surfaceHeight(baseHeight: base, at: .init(x: center.x, z: center.y)) ?? base
    let fields = SanctuaryWaterBody.allCases.compactMap {
      terrain.waterField(for: $0, at: center, bedHeight: bed)
    }
    if fields.contains(where: { $0.signedCoverage >= -step && $0.signedCoverage <= 2 + step }) {
      // Coverage can be limited by vertical clearance, not horizontal distance. Inspect
      // variation before refining so a broad dry shelf cannot become millions of vertices.
      let points = [center, low, SIMD2(high.x, low.y), high, SIMD2(low.x, high.y)]
      let samples = points.map { terrain.resolvedWaterField(at: $0, garden: garden) }
      let wet = samples.compactMap { $0 }
      if !wet.isEmpty {
        let crossesBank = wet.count != samples.count
        let minimumCoverage = wet.map(\.signedCoverage).min()!
        let maximumCoverage = wet.map(\.signedCoverage).max()!
        let crossesTransition = minimumCoverage < 2 && maximumCoverage - minimumCoverage > 0.25
        var curved = false
        if let middle = samples[0], wet.count == samples.count {
          let average = samples.dropFirst().reduce(Float(0)) { $0 + $1!.surfaceHeight } * 0.25
          curved = abs(middle.surfaceHeight - average) > 0.03
        }
        if crossesBank || crossesTransition || curved {
          result = wet.contains(where: { $0.body == .lake || $0.body == .ocean }) ? 16 : 4
        }
      }
    }
    for patch in waterPatches {
      let center = SIMD2(patch.center.x, patch.center.z)
      let closest = simd_min(simd_max(center, low), high)
      if length_squared(center - closest) > patch.radius * patch.radius { continue }
      let spacing = min(step, max(0.125, patch.radius * 0.25))
      while Float(result) * spacing < step { result *= 2 }
    }
    return result
  }

  private struct WaterVertex {
    let point: SIMD2<Float>
    let height: Float
    let color: V3
  }

  /// Evaluate actual surface heights at shared vertices and bisect wet/dry edges. The
  /// previous center-height quads made a staircase even where the source was continuous.
  private static func clippedWaterMesh(
    terrain: Terrain, garden: HabitatGarden?, lattice: Mesh, largeBodiesOnly: Bool = false
  ) -> Mesh {
    var mesh = Mesh()
    var samples: [SIMD2<Float>: WaterVertex?] = [:]
    var indices: [SIMD2<Float>: UInt32] = [:]
    func sample(_ p: SIMD2<Float>) -> WaterVertex? {
      if let cached = samples[p] { return cached }
      let field = terrain.resolvedWaterField(at: p, garden: garden)
      var value: WaterVertex?
      if let field, field.isWet,
        !largeBodiesOnly || field.body == .lake || field.body == .ocean {
        value = WaterVertex(point: p, height: field.surfaceHeight,
          color: field.body.map(waterColor) ?? V3(0.31, 0.58, 0.62))
      }
      samples[p] = .some(value)
      return value
    }
    func shore(_ inside: WaterVertex, _ outside: SIMD2<Float>) -> WaterVertex {
      var wet = inside, dry = outside
      // 4 m detail edges converge below two centimetres; distant edges use the same
      // absolute tolerance so a shared boundary does not depend on triangle direction.
      for _ in 0..<16 {
        if length_squared(wet.point - dry) <= 0.0001 { break }
        let middle = (wet.point + dry) * 0.5
        if let value = sample(middle) { wet = value } else { dry = middle }
      }
      return wet
    }
    func vertex(_ value: WaterVertex) -> UInt32 {
      if let i = indices[value.point] { return i }
      let i = UInt32(mesh.vertices.count)
      mesh.vertices.append(Vertex(V3(value.point.x, value.height, value.point.y), .zero, value.color))
      indices[value.point] = i
      return i
    }
    func triangle(_ points: [SIMD2<Float>]) {
      let values = points.map(sample)
      guard values.contains(where: { $0 != nil }) else { return }
      var polygon: [WaterVertex] = []
      for i in 0..<3 {
        let next = (i + 1) % 3
        if let value = values[i] {
          polygon.append(value)
          if values[next] == nil { polygon.append(shore(value, points[next])) }
        } else if let value = values[next] { polygon.append(shore(value, points[i])) }
      }
      guard polygon.count >= 3 else { return }
      let first = vertex(polygon[0])
      for i in 1..<(polygon.count - 1) {
        let b = vertex(polygon[i]), c = vertex(polygon[i + 1])
        if first != b && b != c && c != first { mesh.indices += [first, b, c] }
      }
    }
    for i in stride(from: 0, to: lattice.indices.count, by: 3) {
      let points = (0..<3).map { j -> SIMD2<Float> in
        let p = lattice.vertices[Int(lattice.indices[i + j])].position
        return SIMD2(p.x, p.z)
      }
      triangle(points)
    }
    // Shared area-weighted normals preserve sloping creek/wetland continuity.
    for i in stride(from: 0, to: mesh.indices.count, by: 3) {
      let ids = (0..<3).map { Int(mesh.indices[i + $0]) }
      let p = ids.map { j -> V3 in
        let value = mesh.vertices[j].position
        return V3(value.x, value.y, value.z)
      }
      let n = cross(p[1] - p[0], p[2] - p[0])
      for j in ids { mesh.vertices[j].normal += SIMD4(n, 0) }
    }
    for i in mesh.vertices.indices {
      let value = mesh.vertices[i].normal
      let n = V3(value.x, value.y, value.z)
      mesh.vertices[i].normal = SIMD4(length_squared(n) > 0.000001 ? normalize(n) : V3(0, 1, 0), 0)
    }
    return mesh
  }

  static func chunkBatches(
    terrain: Terrain, plan: SanctuaryChunkPlan, garden: HabitatGarden?,
    templates: BiomePresentationTemplates, boulders: BoulderArrangements? = nil
  ) -> [SceneBatch] {
    let id = plan.key.id
    var batches: [SceneBatch] = [
      SceneBatch(
        name: "World terrain \(id)",
        mesh: terrainMesh(terrain: terrain, key: plan.key, garden: garden),
        instances: [Instance(kind: BiomeGroundAppearance.materialKind)])
    ]
    let water = waterMesh(terrain: terrain, key: plan.key, garden: garden)
    if !water.indices.isEmpty {
      batches.append(
        SceneBatch(
          name: "World water \(id)", mesh: water, instances: [Instance(kind: 3)],
          roughness: 0.3, doubleSided: true, metallic: 0.04))
    }

    var trunks: [Instance] = []
    var broadCrowns: [Instance] = []
    var willows: [Instance] = []
    var palms: [Instance] = []
    var conifers: [Instance] = []
    var cacti: [Instance] = []
    var stones: [Instance] = []
    var driftwood: [Instance] = []
    var accentGrass: [GrassBlade] = []
    var flowers = Mesh()
    for source in plan.features {
      guard let resolved = SanctuaryLayout.resolveFeature(
        source, terrain: terrain, garden: garden, boulders: boulders)
      else { continue }
      let feature = resolved.source
      let position = resolved.position
      switch feature.kind {
      case .broadleaf:
        trunks.append(
          Instance(position: position, scale: resolved.coreScale, yaw: resolved.yaw,
            tint: V3(0.38, 0.27, 0.18), kind: 4))
        broadCrowns.append(
          Instance(position: position, scale: resolved.canopyScale, yaw: resolved.yaw,
            tint: V3(0.37, 0.55, 0.27), kind: 2))
      case .willow, .palm:
        // Neutral tint retains reviewed source colors. This exact model matrix is copied to
        // wood and foliage, and collision consumes the same resolved uniform scale and yaw.
        let instance = Instance(position: position, scale: resolved.coreScale, yaw: resolved.yaw,
          kind: 4)
        if feature.kind == .willow { willows.append(instance) } else { palms.append(instance) }
      case .conifer:
        conifers.append(
          Instance(position: position, scale: resolved.coreScale, yaw: resolved.yaw, kind: 4))
      case .cactus:
        cacti.append(
          Instance(position: position, scale: resolved.coreScale, yaw: resolved.yaw,
            tint: V3(0.31, 0.48, 0.29), kind: 3))
      case .boulder, .stoneSpire, .seaStack, .waystone:
        let tint = feature.isDiscovery ? V3(0.76, 0.57, 0.34) : V3(0.48, 0.52, 0.50)
        stones.append(
          Instance(
            position: position, scale: resolved.coreScale, yaw: resolved.yaw,
            tint: tint, kind: 5))
      case .driftwood:
        driftwood.append(
          Instance(
            position: position, scale: resolved.coreScale, yaw: resolved.yaw,
            tint: V3(0.43, 0.34, 0.25), kind: 4))
      case .reedCluster:
        for j in 0..<18 {
          let angle = feature.yaw + Float(j) * 2.399
          let radius = 0.18 * sqrt(Float(j))
          accentGrass.append(
            GrassBlade(
              anchor: position + V3(cos(angle) * radius, -0.015, sin(angle) * radius),
              angle: angle, height: 0.75 + 0.045 * Float(j % 7), width: 0.04,
              color: V3(0.42, 0.51, 0.22)))
        }
      case .flowerPatch:
        for j in 0..<12 {
          let angle = feature.yaw + Float(j) * 2.399
          let radius = 0.23 * sqrt(Float(j))
          let center = position + V3(cos(angle) * radius, 0.34, sin(angle) * radius)
          let color = j.isMultiple(of: 2) ? V3(0.90, 0.69, 0.57) : V3(0.83, 0.80, 0.47)
          for petal in 0..<5 {
            let a = Float(petal) * 2 * .pi / 5
            let b = Float(petal + 1) * 2 * .pi / 5
            flowers.triangle(
              Vertex(center, V3(0, 1, 0), color, wind: 1),
              Vertex(center + V3(cos(a) * 0.16, 0.025, sin(a) * 0.16), V3(0, 1, 0), color, wind: 1),
              Vertex(center + V3(cos(b) * 0.16, 0.025, sin(b) * 0.16), V3(0, 1, 0), color, wind: 1))
          }
        }
      }
    }

    func append(_ name: String, _ mesh: Mesh, _ instances: [Instance], _ roughness: Float = -1) {
      guard !instances.isEmpty else { return }
      batches.append(
        SceneBatch(
          name: "\(name) \(id)", mesh: mesh, instances: instances,
          lodMeshes: [MeshProcessing.simplify(mesh, ratio: 0.28, error: 0.08)], roughness: roughness))
    }
    func appendBotanical(_ templates: [SceneBatch], _ instances: [Instance]) {
      guard !instances.isEmpty else { return }
      for template in templates {
        var batch = template
        batch.name = "\(template.name) \(id)"
        let material = template.instances[0].tint.w
        batch.instances = instances.map { source in
          var instance = source
          instance.tint.w = material
          return instance
        }
        batches.append(batch)
      }
    }
    appendBotanical(templates.willow, willows)
    appendBotanical(templates.palm, palms)
    appendBotanical(templates.conifer, conifers)
    append("Regional trunks", templates.trunk, trunks)
    append("Regional crowns", templates.broadCrown, broadCrowns)
    append("Desert cactus", templates.cactus, cacti, 0.82)
    append("Regional landmarks", templates.stone, stones, 0.88)
    append("Coastal driftwood", templates.driftwood, driftwood, 0.9)
    if !flowers.indices.isEmpty {
      batches.append(
        SceneBatch(
          name: "Regional flowers \(id)", mesh: flowers, instances: [Instance(kind: 3)],
          roughness: 0.86, doubleSided: true))
    }

    var grass: [GrassBlade] = accentGrass
    var rng = SeededRandom(seed: 0xA11C_E123 ^ UInt64(bitPattern: Int64(plan.key.x * 97 + plan.key.z * 193)))
    let bladeCount: Int
    switch plan.primaryBiome {
    case .meadow: bladeCount = 1_800
    case .woodland, .rainforest: bladeCount = 1_050
    case .creek, .wetland, .lake: bladeCount = 900
    case .coast, .tidepool: bladeCount = 420
    case .alpine: bladeCount = 300
    case .desert, .ocean: bladeCount = 90
    }
    for _ in 0..<bladeCount {
      let p = plan.key.minimum + SIMD2(
        rng.range(0, SanctuaryTerrainChunkKey.size), rng.range(0, SanctuaryTerrainChunkKey.size))
      guard SanctuaryGeography.bounds.contains(p), terrain.water(at: p) == nil else { continue }
      let y = composedHeight(terrain, garden: garden, x: p.x, z: p.y)
      let reed = plan.primaryBiome == .creek || plan.primaryBiome == .wetland
      let color = reed
        ? V3(0.42, 0.50, 0.22)
        : (plan.primaryBiome == .meadow ? V3(0.52, 0.62, 0.25) : V3(0.31, 0.48, 0.23))
      let blade = GrassBlade(
        anchor: V3(p.x, y - 0.015, p.y), angle: rng.range(0, 2 * .pi),
        height: reed ? rng.range(0.8, 1.8) : rng.range(0.22, 0.62),
        width: reed ? rng.range(0.025, 0.055) : rng.range(0.018, 0.045), color: color)
      if plan.primaryBiome == .meadow {
        grass.append(contentsOf: BiomeVegetationDesign.meadowGrassTuft(blade))
      } else {
        grass.append(blade)
      }
    }
    if !grass.isEmpty {
      batches.append(
        SceneBatch(
          name: "Regional groundcover \(id)", mesh: Mesh(), instances: [Instance(kind: 3)],
          grass: grass, roughness: 0.9, doubleSided: true))
    }
    return batches
  }
}
