import FieldCore
import Foundation
import simd

/// Game-owned soft-soil eligibility. Appearance and imprint geometry belong to the project.
public struct SanctuaryGroundSurface: Equatable, Sendable {
  public let height: Float
  public let strength: Float
}

extension SanctuaryWorld {
  /// A conservative soil contact query, shared by every sample of an imprint.
  /// Existing traces disappear if water or construction now covers their support.
  /// Weather defaults to the saved contact time, so a later shower does not
  /// retroactively create an impression beneath an old dry-ground footstep.
  public func groundTraceSupport(
    event: GroundInfluenceEvent, x: Float, z: Float,
    weather weatherOverride: SanctuaryClimate.Sample? = nil
  ) -> SanctuaryGroundSurface? {
    let point = SIMD2(x, z)
    guard event.kind == .foot, x.isFinite, z.isFinite,
      SanctuaryGeography.bounds.contains(point),
      distance(point, SIMD2(event.end.x, event.end.z)) <= 2.5
    else { return nil }
    let height = groundHeight(x, z)
    guard height.isFinite, abs(height - event.end.y) <= min(0.3, event.supportTolerance),
      localWaterHeight(x, z).map({ $0 < height - 0.005 }) ?? true
    else { return nil }
    for fact in controller.state.buildings.collisionFacts + SanctuaryHome.construction.collisionFacts {
      if fact.contains(.init(x: x, y: fact.top, z: z)), fact.top >= height - 0.03 {
        return nil
      }
    }

    // Four composed support probes capture garden edits as well as base terrain.
    // The environment surface path itself performs no terrain/upwind probes.
    let spacing: Float = 0.12
    let dx = groundHeight(x + spacing, z) - groundHeight(x - spacing, z)
    let dz = groundHeight(x, z + spacing) - groundHeight(x, z - spacing)
    let grade = min(4, length(SIMD2(dx, dz)) / (2 * spacing))
    let weather = weatherOverride ?? SanctuaryClimate.sample(
      elapsedPlaySeconds: event.endTime, seed: controller.state.creatureState.seed)
    let environment = SanctuaryEnvironmentField(terrain: world.terrain)
    guard let surface = try? environment.surfaceSample(
      at: point, supportingHeightMetres: height,
      gradeRisePerRun: grade, weather: weather), !surface.isWater
    else { return nil }
    var softness = surface.softSoilSuitability

    // Preserve the established dry two-metre garden shore around authored pools.
    for patch in (controller.state.garden ?? HabitatGarden()).patches where patch.planting == .shallowWater {
      let shoreDistance = distance(point, SIMD2(patch.center.x, patch.center.z)) - patch.radius
      if (0...2).contains(shoreDistance) { softness = max(softness, 1 - shoreDistance / 2) }
    }
    guard softness >= 0.2 else { return nil }
    return SanctuaryGroundSurface(height: height, strength: min(1, softness))
  }
}
