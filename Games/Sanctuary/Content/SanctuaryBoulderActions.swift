import Foundation
import FieldCore
import SimulationCore
import simd

extension SanctuaryWorld {
  func boulderControl(_ id: String) throws -> String {
    if id == "undoBoulder" {
      try controller.editLiving { state in
        var boulders = state.movedBoulders
        try boulders.apply(.undo, expectedRevision: boulders.revision)
        state.boulders = boulders
      }
      updateCollisionStreaming(at: SIMD2(position.x, position.z))
      return ""
    }
    guard let helper = nearbyAnimal, helper.lifeStage == .adult,
      helper.capabilities.contains(.moveBoulders), helper.companion.helpEligible else {
      throw SimulationFailure.invalid("A strong, willing companion can help move a boulder")
    }
    let player = SIMD2(position.x, position.z)
    updateCollisionStreaming(at: player)
    let geography = SanctuaryGeography()
    let arrangements = controller.state.movedBoulders
    let direction = SIMD2(sin(yaw), -cos(yaw))
    let candidates = SanctuaryTerrainChunkKey.neighborhood(around: player)
      .flatMap { geography.plan(for: $0).features }.filter { feature in
        guard feature.kind == .boulder else { return false }
        let point = arrangements.resolvedCoordinate(for: feature)
        let offset = point - player
        return length(offset) <= 12 && length(offset) > 0.1
          && dot(normalize(offset), direction) > 0.65
          && distance(helper.position, point) <= 12
      }
    guard let feature = candidates.min(by: {
      distance(arrangements.resolvedCoordinate(for: $0), player)
        < distance(arrangements.resolvedCoordinate(for: $1), player)
    }) else { throw SimulationFailure.invalid("Face a nearby boulder with your companion beside you") }
    let origin = arrangements.resolvedCoordinate(for: feature)
    let radius = feature.scale * 1.3
    let startHeight = groundHeight(origin.x, origin.y)
    for step in 1...12 {
      let point = origin + direction * (2 * Float(step) / 12)
      let ground = groundHeight(point.x, point.y)
      guard SanctuaryGeography.bounds.contains(point), abs(ground - startHeight) < 1.5,
        (localWaterHeight(point.x, point.y) ?? ground) <= ground + 0.5 else {
        throw SimulationFailure.invalid("The boulder needs a firm, gently sloping place to rest")
      }
      let sample = V3(point.x, ground + radius * 0.6, point.y)
      for solid in world.solids {
        if solid.name == "boulder" && distance(SIMD2(solid.position.x, solid.position.z), origin) < 0.01 { continue }
        if solid.value(sample) < radius { throw SimulationFailure.invalid("The boulder's path is obstructed") }
      }
      for offset in [SIMD2<Float>.zero, SIMD2(radius, 0), SIMD2(-radius, 0), SIMD2(0, radius), SIMD2(0, -radius)] {
        let probe = point + offset
        if constructionFacts.contains(where: { $0.contains(.init(x: probe.x, y: ground + 0.5, z: probe.y)) }) {
          throw SimulationFailure.invalid("The boulder would overlap a construction")
        }
      }
      guard !controller.state.population.ecology.collisionFacts.contains(where: {
        distance($0.center, point) < $0.radius + radius
      }) else { throw SimulationFailure.invalid("The boulder would obstruct an animal's habitat work") }
    }
    try controller.editLiving { state in
      var boulders = state.movedBoulders
      try boulders.apply(.push(featureID: feature.id, direction: direction, distance: 2,
        helperID: helper.id), expectedRevision: boulders.revision)
      // The assistance credit belongs to this concrete, revisioned push. Both
      // changes live in one save-backed candidate so a duplicate credit or
      // failed write cannot publish either a displacement or a memory.
      var population = state.population
      try population.recordAssistance(
        animalID: helper.id, outcomeID: "\(feature.id):\(boulders.revision)",
        kind: .boulderMovement, expectedRevision: population.revision)
      state.boulders = boulders
      state.wildlife = population
    }
    updateCollisionStreaming(at: player)
    return ""
  }
}
