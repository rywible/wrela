import FieldCore
import Foundation
import simd

/// One resolved source transform shared by vegetation meshes and near collision.
/// Garden and saved construction exclusions do not alter procedural identities or saves.
public struct SanctuaryVegetationPlacement {
  public let record: SanctuaryVegetationRecord
  public let position: V3
  public let solid: Solid?

  public static func resolve(
    _ record: SanctuaryVegetationRecord, terrain: Terrain, garden: HabitatGarden?,
    decision: SanctuaryVegetationLocalEditDecision, constructionFacts: [PersonalConstruction.CollisionFact], treeTrunk: Shape
  ) -> Self? {
    guard decision.sourceID == record.id, decision.isIncluded,
      terrain.water(at: record.coordinate) == nil else { return nil }
    let x = record.coordinate.x, z = record.coordinate.y
    let base = terrain.height(x, z)
    let location = HabitatGarden.Location(x: x, z: z)
    let height = garden?.surfaceHeight(baseHeight: base, at: location) ?? base
    if record.species != .reeds,
      let water = garden?.waterSurfaceHeight(baseHeight: base, at: location), water > height + 0.01
    { return nil }
    let source: Shape?
    switch record.species {
    case .broadleaf: source = treeTrunk
    case .willow: source = SanctuaryBotanicalTrunk.willowCollision
    case .palm: source = SanctuaryBotanicalTrunk.palmCollision
    case .conifer: source = SanctuaryBotanicalTrunk.coniferCollision
    case .cactus: source = SanctuaryLayout.cactusField
    case .reeds: source = nil
    }
    let margin: Float = source.map {
      max(max(abs($0.bounds.min.x), abs($0.bounds.max.x)),
        max(abs($0.bounds.min.z), abs($0.bounds.max.z))) * record.scale
    } ?? 0.18 * record.scale
    let top = height + (source?.bounds.max.y ?? 1) * record.scale
    for fact in constructionFacts where fact.top >= height && fact.bottom <= top {
      let dx = x - fact.center.x, dz = z - fact.center.z
      let c = cos(fact.yawRadians), s = sin(fact.yawRadians)
      if abs(dx * c - dz * s) <= fact.halfExtents.x + margin,
        abs(dx * s + dz * c) <= fact.halfExtents.z + margin
      { return nil }
    }
    let position = V3(x, height, z)
    let solid = source.map {
      Solid(shape: $0.stretched(V3(repeating: record.scale)).rotated(
        simd_quatf(angle: record.yaw, axis: V3(0, 1, 0))), position: position,
        scale: 1, name: record.id)
    }
    return Self(record: record, position: position, solid: solid)
  }
}
