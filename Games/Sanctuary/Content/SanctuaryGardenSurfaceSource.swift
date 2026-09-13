import Foundation
import simd

/// The saved contributions consumed by terrain, water, substrate and regional trunk support.
/// Decorative planting still publishes the complete HabitatGarden; it does not change these
/// surfaces. This value deliberately compares source facts rather than the save revision.
public struct SanctuaryGardenSurfaceSource: Equatable, Sendable {
  public let terrainPatches: [HabitatGarden.TerrainPatch]
  public let waterPatches: [HabitatGarden.Patch]

  public init(garden: HabitatGarden?) {
    terrainPatches = garden?.orderedTerrainPatches ?? []
    waterPatches = garden?.patches.filter { $0.planting == .shallowWater } ?? []
  }

  public var coveredChunkKeys: Set<SanctuaryTerrainChunkKey> {
    var result: Set<SanctuaryTerrainChunkKey> = []
    func insert(_ center: HabitatGarden.Location, radius: Float) {
      let lower = SanctuaryTerrainChunkKey(containing: SIMD2(center.x - radius, center.z - radius))
      let upper = SanctuaryTerrainChunkKey(containing: SIMD2(center.x + radius, center.z + radius))
      for z in lower.z...upper.z { for x in lower.x...upper.x {
        let key = SanctuaryTerrainChunkKey(x: x, z: z)
        if key.intersectsWorld { result.insert(key) }
      } }
    }
    // Include the terrain compiler's quarter-metre normal probes and the authored
    // substrate's sixteen-metre water-bank blend, including across canonical chunk edges.
    for patch in terrainPatches { insert(patch.center, radius: patch.radius + 0.25) }
    for patch in waterPatches { insert(patch.center, radius: patch.radius + 16) }
    return result
  }

  /// Both removed and added source footprints must be refreshed on undo, restore or a
  /// same-revision save-branch replacement. Decoration-only changes return no dirty keys.
  public func affectedChunkKeys(comparedTo previous: Self) -> Set<SanctuaryTerrainChunkKey> {
    self == previous ? [] : coveredChunkKeys.union(previous.coveredChunkKeys)
  }
}

/// Exact source receipt for the existing regional worker's intent. Unlike surface dirtiness,
/// equality includes decoration, saved identifiers/history and all placement source facts.
/// Two restored branches may share revision numbers and still require a new generation.
public struct SanctuaryRegionalSourceReceipt: Equatable, Sendable {
  public let center: SanctuaryTerrainChunkKey
  public let garden: HabitatGarden?
  public let boulders: BoulderArrangements?
  public let construction: PersonalConstruction?

  public init(center: SanctuaryTerrainChunkKey, garden: HabitatGarden?,
    boulders: BoulderArrangements?, construction: PersonalConstruction?) {
    self.center = center
    self.garden = garden
    self.boulders = boulders
    self.construction = construction
  }
}

/// Presentation exclusion for immutable natural ground-cover samples. Saved sculpt footprints
/// are cleared instead of moving old roots; removing an edit reveals the same source samples.
/// Decoration-only planting deliberately does not enter this mask or terrain dirtiness.
public struct SanctuaryGroundCoverMask: Equatable, Sendable {
  public let surface: SanctuaryGardenSurfaceSource
  public let constructionFacts: [PersonalConstruction.CollisionFact]
  private let boxes: [Box]

  private struct Box: Equatable, Sendable {
    let fact: PersonalConstruction.CollisionFact
    let cosine: Float
    let sine: Float
  }

  public init(garden: HabitatGarden?, construction: PersonalConstruction?) {
    surface = SanctuaryGardenSurfaceSource(garden: garden)
    constructionFacts = construction?.collisionFacts ?? []
    boxes = constructionFacts.map { Box(fact: $0, cosine: cos($0.yawRadians), sine: sin($0.yawRadians)) }
  }

  public var isEmpty: Bool {
    surface.terrainPatches.isEmpty && surface.waterPatches.isEmpty && boxes.isEmpty
  }

  /// Radius bounds the whole leaf/flower, including a small existing wind/contact margin.
  /// Construction uses its actual vertical interval, so elevated roofs do not clear grass
  /// below them. This is conservative presentation clipping, never terrain collision.
  public func excludes(root: SIMD3<Float>, height: Float, radius: Float) -> Bool {
    guard root.x.isFinite, root.y.isFinite, root.z.isFinite,
      height.isFinite, radius.isFinite, height >= 0, radius >= 0 else { return true }
    func overlaps(_ center: HabitatGarden.Location, _ patchRadius: Float) -> Bool {
      let dx = root.x - center.x, dz = root.z - center.z
      let reach = patchRadius + radius
      return dx * dx + dz * dz <= reach * reach
    }
    for patch in surface.terrainPatches where patch.amount > 0 {
      if overlaps(patch.center, patch.radius) { return true }
    }
    for patch in surface.waterPatches {
      if overlaps(patch.center, patch.radius) { return true }
    }
    for box in boxes {
      let fact = box.fact
      guard fact.top >= root.y - 0.02, fact.bottom <= root.y + height else { continue }
      let dx = root.x - fact.center.x, dz = root.z - fact.center.z
      if abs(dx * box.cosine - dz * box.sine) <= fact.halfExtents.x + radius,
        abs(dx * box.sine + dz * box.cosine) <= fact.halfExtents.z + radius { return true }
    }
    return false
  }
}
