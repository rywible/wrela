import FieldCore
import Foundation
import simd

public struct Solid {
  public var shape: Shape
  public var position: V3
  public var scale: Float
  public var name: String
  public init(shape: Shape, position: V3, scale: Float, name: String) {
    self.shape = shape
    self.position = position
    self.scale = scale
    self.name = name
  }
  public func value(_ p: V3) -> Float { shape.value(at: (p - position) / scale) * scale }
}
public struct Placement {
  public var position: V3
  public var scale: V3
  public var yaw: Float
  public var color: V3
  public var kind: Float
  public init(position: V3, scale: V3, yaw: Float = 0, tint: V3, kind: Float) {
    self.position = position
    self.scale = scale
    self.yaw = yaw
    self.color = tint
    self.kind = kind
  }
}

public enum SanctuaryFeaturePrimitive: Sendable {
  case tree, cactus, stone, driftwood, groundcover
}

public struct SanctuaryResolvedFeature: Sendable {
  public let source: SanctuaryChunkFeature
  public let position: V3
  public let yaw: Float
  public let coreScale: V3
  public let canopyScale: V3
  public let primitive: SanctuaryFeaturePrimitive
}
/// Shared CPU layout: rendering and collision consume the same placement sequence.
public struct SanctuaryLayout {
  public static let podMetres: Float = 0.035
  public static var podUnitScale: Float {
    podMetres / (podShape.bounds.max.y - podShape.bounds.min.y)
  }
  public let terrain: Terrain
  public var trunks: [Placement] = [], crowns: [Placement] = [], rocks: [Placement] = [],
    solids: [Solid] = []
  public var collisionGrid = SpatialBoundsGrid(bounds: [])
  public private(set) var streamedCollisionKeys: Set<SanctuaryTerrainChunkKey> = []
  private var cabinSolidCount = 0
  private var regionalSolidCache: [SanctuaryTerrainChunkKey: [Solid]] = [:]
  private var vegetationSolidCache: [SanctuaryTerrainChunkKey: [Solid]] = [:]
  private let vegetationTrunkSource: Shape
  private var immediateVegetationRecords: [SanctuaryTerrainChunkKey: [SanctuaryVegetationRecord]] = [:]
  private var immediateCollisionKeys: Set<SanctuaryTerrainChunkKey> = []
  private var immediateGardenSource: SanctuaryGardenSurfaceSource?
  private var immediateGarden: HabitatGarden?
  private var immediateConstruction: PersonalConstruction?
  private var immediateConstructionFacts: [PersonalConstruction.CollisionFact]?
  private var immediateBoulders: BoulderArrangements?
  public var immediateVegetationCachedChunks: Int { immediateVegetationRecords.count }
  public private(set) var immediateVegetationGeneratedChunksLastUpdate = 0
  public private(set) var immediateVegetationResolvedChunksLastUpdate = 0
  public var remainingRandom: SeededRandom
  public let seed: UInt64 = 82317
  public static var arch: Shape {
    Shape.capsule(V3(-3, 0, 0), V3(-2, 8, 0), 0.85).blended(
      .capsule(V3(3, 0, 0), V3(2, 8, 0), 0.85), radius: 0.5
    ).blended(.capsule(V3(-2, 8, 0), V3(2, 8, 0), 1), radius: 0.75)
  }
  public var gatePosition: V3 {
    V3(terrain.pathX(-67), terrain.height(terrain.pathX(-67), -67), -67)
  }
  public init(parameters: [String: Float] = [:], terrain: Terrain = Terrain()) {
    self.terrain = terrain
    remainingRandom = SeededRandom(seed: 82317)
    let trunk = Self.treeFields(parameters).0
    vegetationTrunkSource = trunk
    let rockShape = Self.stoneField

    func encounterClearing(_ x: Float, _ z: Float) -> Bool {
      let p = SIMD2(x, z)
      return distance(p, Expedition.den) < 6 || distance(p, Expedition.home) < 7
        || Expedition.signs.contains { distance(p, $0) < 2 }
    }
    var rng = SeededRandom(seed: seed)
    for _ in 0..<260 {
      let x = rng.range(-105, 105)
      let z = rng.range(-125, 65)
      if encounterClearing(x, z) { continue }
      let pathDistance = abs(x - terrain.pathX(z))
      if pathDistance < 5 || (abs(x) < 13 && z > 9 && z < 33) { continue }
      let s = rng.range(1.1, 2.3)
      let p = V3(x, terrain.height(x, z) - 0.15, z)
      let yaw = rng.range(0, 6.28)
      trunks.append(
        Placement(
          position: p, scale: V3(repeating: s), yaw: yaw, tint: V3(0.31, 0.25, 0.18), kind: 4))
      let gold = rng.next()
      let color =
        gold < 0.22
        ? V3(0.73, 0.70, 0.32)
        : V3(rng.range(0.33, 0.48), rng.range(0.57, 0.70), rng.range(0.27, 0.39))
      crowns.append(Placement(position: p, scale: V3(repeating: s), yaw: yaw, tint: color, kind: 2))
      solids.append(Solid(shape: trunk, position: p, scale: s, name: "Tree"))
    }
    for _ in 0..<95 {
      let x = rng.range(-75, 75)
      let z = rng.range(-105, 50)
      if abs(x - terrain.pathX(z)) < 3.4 || encounterClearing(x, z) { continue }
      let s = rng.range(0.5, 2.3)
      let p = V3(x, terrain.height(x, z), z)
      rocks.append(
        Placement(position: p, scale: V3(repeating: s), tint: V3(0.53, 0.57, 0.51), kind: 5))
      solids.append(Solid(shape: rockShape, position: p, scale: s, name: "Stone"))
    }

    remainingRandom = rng
    solids.append(Solid(shape: Self.arch, position: gatePosition, scale: 1, name: "The old gate"))
    cabinSolidCount = solids.count
    rebuildCollisionGrid()
  }

  public var worldBounds: SanctuaryMapBounds { SanctuaryGeography.bounds }

  /// One source placement for both mesh instances and collision. A nil result means the natural
  /// water policy omitted the feature, so it must exist in neither presentation nor collision.
  public static func resolveFeature(
    _ feature: SanctuaryChunkFeature, terrain: Terrain, garden: HabitatGarden? = nil,
    boulders: BoulderArrangements? = nil
  ) -> SanctuaryResolvedFeature? {
    let coordinate = boulders?.resolvedCoordinate(for: feature) ?? feature.coordinate
    let displacedBoulder = feature.kind == .boulder
      && boulders?.displacement(for: feature.id) != nil
    if terrain.water(at: coordinate) != nil && feature.kind != .seaStack && !displacedBoulder {
      return nil
    }
    let baseY = terrain.height(coordinate.x, coordinate.y)
    let y = garden?.surfaceHeight(
      baseHeight: baseY, at: .init(x: coordinate.x, z: coordinate.y)) ?? baseY
    let coreScale: V3
    let canopyScale = V3(repeating: feature.scale)
    let primitive: SanctuaryFeaturePrimitive
    switch feature.kind {
    case .broadleaf:
      coreScale = V3(0.75, 1.25, 0.75) * feature.scale
      primitive = .tree
    case .willow, .palm:
      coreScale = canopyScale
      primitive = .tree
    case .conifer:
      coreScale = canopyScale
      primitive = .tree
    case .cactus:
      coreScale = V3(repeating: feature.scale)
      primitive = .cactus
    case .boulder, .waystone:
      coreScale = V3(repeating: feature.scale)
      primitive = .stone
    case .stoneSpire, .seaStack:
      coreScale = V3(1, 2.8, 1) * feature.scale
      primitive = .stone
    case .driftwood:
      coreScale = V3(repeating: feature.scale)
      primitive = .driftwood
    case .reedCluster, .flowerPatch:
      coreScale = V3(repeating: feature.scale)
      primitive = .groundcover
    }
    return SanctuaryResolvedFeature(
      source: feature, position: V3(coordinate.x, y, coordinate.y), yaw: feature.yaw,
      coreScale: coreScale, canopyScale: canopyScale, primitive: primitive)
  }

  public static func collisionShape(for feature: SanctuaryResolvedFeature) -> Shape? {
    let source: Shape
    switch feature.primitive {
    case .tree:
      switch feature.source.kind {
      case .willow: source = SanctuaryBotanicalTrunk.willowCollision
      case .palm: source = SanctuaryBotanicalTrunk.palmCollision
      case .conifer: source = SanctuaryBotanicalTrunk.coniferCollision
      default: source = treeFields().0
      }
    case .cactus: source = cactusField
    case .stone: source = stoneField
    case .driftwood: source = driftwoodField
    case .groundcover: return nil
    }
    return source.stretched(feature.coreScale).rotated(
      simd_quatf(angle: feature.yaw, axis: V3(0, 1, 0)))
  }

  /// Replaces only streamed regional collision. Cabin placements and their source order stay
  /// unchanged, preserving the existing expedition and deterministic tests.
  public mutating func setStreamedCollision(
    plans: [SanctuaryChunkPlan], garden: HabitatGarden? = nil,
    boulders: BoulderArrangements? = nil, affectedKeys: Set<SanctuaryTerrainChunkKey>? = nil
  ) {
    streamedCollisionKeys = Set(plans.map(\.key))
    regionalSolidCache = regionalSolidCache.filter { streamedCollisionKeys.contains($0.key) }
    for plan in plans {
      if regionalSolidCache[plan.key] != nil, let affectedKeys, !affectedKeys.contains(plan.key) { continue }
      var compiled: [Solid] = []
      for source in plan.features {
        guard let feature = Self.resolveFeature(
          source, terrain: terrain, garden: garden, boulders: boulders),
          let shape = Self.collisionShape(for: feature)
        else { continue }
        compiled.append(
          Solid(
            shape: shape, position: feature.position, scale: 1,
            name: feature.source.kind.rawValue))
      }
      regionalSolidCache[plan.key] = compiled
    }
    rebuildStreamedSolids()
  }

  /// CPU hosts resolve the same near population used by native visual publication. Record
  /// generation is cached by canonical chunk; edits only re-resolve affected placements.
  /// All queries complete before mutating collision, so a failed source never installs a
  /// partial near set. Native hosts keep their existing staged GPU publication path.
  public mutating func updateImmediateCollision(
    around point: SIMD2<Float>, garden: HabitatGarden? = nil,
    boulders: BoulderArrangements? = nil, construction: PersonalConstruction? = nil
  ) throws {
    immediateVegetationGeneratedChunksLastUpdate = 0
    immediateVegetationResolvedChunksLastUpdate = 0
    guard point.x.isFinite, point.y.isFinite, SanctuaryGeography.bounds.contains(point) else {
      throw SanctuaryVegetationResidencyError.invalidCoordinate
    }
    let keys = Set(SanctuaryTerrainChunkKey.neighborhood(around: point))
    // Full value receipts catch same-revision restore branches, while unchanged ticks
    // avoid reconstructing ordered terrain patches or construction collision facts.
    guard keys != immediateCollisionKeys || garden != immediateGarden
      || construction != immediateConstruction || boulders != immediateBoulders else { return }
    let surface = SanctuaryGardenSurfaceSource(garden: garden)
    let facts = construction?.collisionFacts ?? []
    if keys == immediateCollisionKeys && surface == immediateGardenSource
      && facts == immediateConstructionFacts && boulders == immediateBoulders {
      immediateGarden = garden
      immediateConstruction = construction
      return
    }
    let orderedKeys = keys.sorted { $0.z == $1.z ? $0.x < $1.x : $0.z < $1.z }
    let population = SanctuaryBiomeVegetationPopulation(habitat: .init(terrain: terrain))
    let mask = SanctuaryVegetationLocalEditMask(garden: garden, construction: construction)
    let gardenDirty = surface.affectedChunkKeys(
      comparedTo: immediateGardenSource ?? SanctuaryGardenSurfaceSource(garden: nil))
    let constructionChanged = facts != immediateConstructionFacts
    var records = immediateVegetationRecords.filter { keys.contains($0.key) }
    var collision = vegetationSolidCache.filter { keys.contains($0.key) }
    var generated = 0
    var resolved = 0
    for key in orderedKeys {
      if records[key] == nil {
        let bounds = SanctuaryMapBounds(
          minimum: simd_max(key.minimum, SanctuaryGeography.bounds.minimum),
          maximum: simd_min(key.maximum, SanctuaryGeography.bounds.maximum))
        records[key] = try population.query(in: bounds).exact
        generated += 1
        collision[key] = nil
      }
      if collision[key] == nil || gardenDirty.contains(key) || constructionChanged {
        collision[key] = records[key]!.compactMap { record in
          SanctuaryVegetationPlacement.resolve(record, terrain: terrain, garden: garden,
            decision: mask.decision(for: record), constructionFacts: facts,
            treeTrunk: vegetationTrunkSource)?.solid
        }
        resolved += 1
      }
    }
    var regionalDirty = keys.subtracting(streamedCollisionKeys).union(gardenDirty.intersection(keys))
    if boulders != immediateBoulders {
      regionalDirty.formUnion(boulders?.sourceChunkKeys ?? [])
      regionalDirty.formUnion(immediateBoulders?.sourceChunkKeys ?? [])
    }
    let geography = SanctuaryGeography()
    setStreamedCollision(plans: orderedKeys.map { geography.plan(for: $0) },
      garden: garden, boulders: boulders, affectedKeys: regionalDirty)
    setVegetationCollision(collision)
    immediateVegetationRecords = records
    immediateCollisionKeys = keys
    immediateGardenSource = surface
    immediateGarden = garden
    immediateConstruction = construction
    immediateConstructionFacts = facts
    immediateBoulders = boulders
    immediateVegetationGeneratedChunksLastUpdate = generated
    immediateVegetationResolvedChunksLastUpdate = resolved
  }

  /// Call with the complete displayed near set. Solids are already resolved off-thread;
  /// unchanged chunks reuse their cached source fields and stable record order.
  public mutating func setVegetationCollision(_ chunks: [SanctuaryTerrainChunkKey: [Solid]]) {
    vegetationSolidCache = chunks
    rebuildStreamedSolids()
  }

  private mutating func rebuildStreamedSolids() {
    if solids.count > cabinSolidCount { solids.removeSubrange(cabinSolidCount...) }
    func ordered(_ cache: [SanctuaryTerrainChunkKey: [Solid]]) -> [Solid] {
      cache.keys.sorted { $0.z == $1.z ? $0.x < $1.x : $0.z < $1.z }.flatMap { cache[$0] ?? [] }
    }
    solids += ordered(regionalSolidCache)
    solids += ordered(vegetationSolidCache)
    rebuildCollisionGrid()
  }

  private mutating func rebuildCollisionGrid() {
    collisionGrid = SpatialBoundsGrid(
      bounds: solids.map {
        let b = $0.shape.bounds
        return Bounds(b.min * $0.scale + $0.position, b.max * $0.scale + $0.position).expanded(
          0.28 / $0.shape.exteriorDistanceScale)
      })
  }
  public static func treeFields(_ parameters: [String: Float] = [:]) -> (Shape, Shape) {
    let h = (parameters["height"] ?? 5.6) / 5.6
    let w = (parameters["width"] ?? 6) / 6
    let r = parameters["trunk"] ?? 0.24
    let spread = parameters["spread"] ?? 1
    let trunk = Shape.capsule(V3(0, 0, 0), V3(0.12, 3.6, 0.1), r)
      .joined(.capsule(V3(0, 2.1, 0), V3(1.2 * spread, 3.4, 0.1), r * 0.54))
      .joined(.capsule(V3(0, 2.4, 0), V3(-spread, 3.65, 0.2), r * 0.58))
    let crown = Shape.sphere(1).stretched(V3(2.05, 0.85, 1.65)).moved(V3(0, 4.2, 0))
      .blended(Shape.sphere(1).stretched(V3(1.5, 0.7, 1.35)).moved(V3(1.4, 3.9, 0.2)), radius: 0.3)
      .blended(
        Shape.sphere(1).stretched(V3(1.65, 0.7, 1.4)).moved(V3(-1.3, 4.3, 0.1)), radius: 0.25
      )
      .blended(
        Shape.sphere(1).stretched(V3(1.45, 0.7, 1.25)).moved(V3(-0.25, 4.95, 0.1)), radius: 0.25
      )
      .blended(
        Shape.sphere(1).stretched(V3(1.3, 0.65, 1.2)).moved(V3(0.1, 4.0, -1.15)), radius: 0.3)
    return (trunk.stretched(V3(w, h, w)), crown.stretched(V3(w, h, w)))
  }
  public static var stoneField: Shape {
    Shape.sphere(0.9).blended(.sphere(0.65).moved(V3(0.55, -0.08, 0.12)), radius: 0.3).cut(
      .box(V3(2, 1, 2)).moved(V3(0, -1.35, 0)))
  }
  public static var cactusField: Shape {
    Shape.capsule(V3(0, 0, 0), V3(0, 3.4, 0), 0.34)
      .blended(.capsule(V3(0, 1.7, 0), V3(1.0, 2.3, 0), 0.2), radius: 0.18)
      .blended(.capsule(V3(1.0, 2.3, 0), V3(1.0, 2.9, 0), 0.2), radius: 0.16)
  }
  public static var driftwoodField: Shape {
    Shape.capsule(V3(-2, 0.3, 0), V3(2, 0.45, 0), 0.28)
  }
  public static var podShape: Shape {
    Shape.sphere(1.15).blended(.sphere(0.72).moved(V3(0, 0.75, 0)), radius: 0.55)
      .blended(.capsule(V3(0, 1.1, 0), V3(0.48, 2.15, 0), 0.18), radius: 0.28)
      .cut(.sphere(0.82).moved(V3(0, 0.24, 0.85)))
  }
}
