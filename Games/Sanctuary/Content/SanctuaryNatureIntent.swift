import FieldCore
import Foundation
import simd

/// A transient brush draft for ordinary Sanctuary nature actions. It contains
/// only source facts needed to preview and validate a possible cast; it is not
/// Codable and is never part of an expedition save.
public struct SanctuaryNatureIntent: Equatable, Sendable {
  public enum Action: Equatable, Sendable {
    case plant(HabitatGarden.Planting)
    case sculpt(HabitatGarden.TerrainOperation)
  }

  public struct SourceRevisions: Equatable, Sendable {
    public let garden: UInt64
    public let construction: UInt64
    public let ecology: UInt64
  }

  public enum Rejection: Equatable, Sendable {
    case staleGarden
    case staleConstruction
    case staleEcology
    case outOfReach
    case water
    case structure
    case occluded
    case unsupportedTarget
  }

  public struct Validation: Equatable, Sendable {
    public let command: HabitatGarden.Command?
    public let target: SanctuaryTargetingResult
    public let rejection: Rejection?
    public var isValid: Bool { rejection == nil }
  }

  public let action: Action
  public let location: HabitatGarden.Location
  public let radius: Float
  public let strength: Float
  /// Smooth captures the composed support elevation present at preparation.
  public let terrainTargetHeight: Float?
  public let origin: V3
  public let direction: V3
  public let reach: Float
  public let sourceRevisions: SourceRevisions
  public let preparedTarget: SanctuaryTargetingResult

  fileprivate init(
    action: Action, location: HabitatGarden.Location, radius: Float, strength: Float,
    terrainTargetHeight: Float?, origin: V3, direction: V3, reach: Float,
    sourceRevisions: SourceRevisions, preparedTarget: SanctuaryTargetingResult
  ) {
    self.action = action
    self.location = location
    self.radius = radius
    self.strength = strength
    self.terrainTargetHeight = terrainTargetHeight
    self.origin = origin
    self.direction = direction
    self.reach = reach
    self.sourceRevisions = sourceRevisions
    self.preparedTarget = preparedTarget
  }

  fileprivate var command: HabitatGarden.Command {
    switch action {
    case let .plant(planting): return .plant(planting, at: location, radius: radius)
    case let .sculpt(operation):
      return .sculpt(operation, at: location, radius: radius, amount: strength,
        targetHeight: operation == .smooth ? terrainTargetHeight : nil)
    }
  }
}

public enum SanctuaryNatureIntentError: LocalizedError, Equatable, Sendable {
  case rejected(SanctuaryNatureIntent.Rejection)

  public var errorDescription: String? {
    switch self {
    case let .rejected(rejection):
      switch rejection {
      case .staleGarden: return "The garden changed. Aim the brush again."
      case .staleConstruction, .staleEcology: return "The target changed. Aim the brush again."
      case .outOfReach: return "Aim within the current brush reach."
      case .water: return "Nature magic needs dry ground, not open water."
      case .structure: return "Aim at open ground, away from a structure."
      case .occluded: return "That brush target is blocked."
      case .unsupportedTarget: return "Aim at a supported patch of ground."
      }
    }
  }
}

extension SanctuaryWorld {
  /// Starts a brush draft from the current production camera target. The target,
  /// brush settings, composed smooth height, and collision-source revisions are
  /// retained so a later cast can reject stale or newly blocked ground.
  public func prepareNatureIntent(_ action: SanctuaryNatureIntent.Action) -> SanctuaryNatureIntent {
    prepareNatureIntent(action, origin: camera.position, direction: camera.forward,
      maxReach: controller.state.craftTools.reach)
  }

  /// Explicit-ray preparation supports the same ordinary target source for a
  /// native preview without adding a second terrain or collision query.
  public func prepareNatureIntent(
    _ action: SanctuaryNatureIntent.Action, origin: V3, direction: V3, maxReach: Float
  ) -> SanctuaryNatureIntent {
    let target = target(origin: origin, direction: direction, maxReach: maxReach)
    let point = target.support?.point ?? target.point
    let tools = controller.state.craftTools
    let radius: Float
    let strength: Float
    switch action {
    case let .plant(planting):
      radius = planting == .grove || planting == .shallowWater
        ? min(16, tools.brushRadius + 1) : tools.brushRadius
      strength = 0
    case let .sculpt(operation):
      radius = min(16, tools.brushRadius * 2)
      strength = operation == .smooth ? min(1, tools.strength) : tools.strength
    }
    let location = HabitatGarden.Location(x: point.x, z: point.z)
    let smoothTarget: Float?
    if case .sculpt(.smooth) = action, point.x.isFinite, point.z.isFinite {
      // The visible/collision support can remain one regional publication
      // behind a just-saved sculpt. Smooth is nevertheless a saved operation,
      // so it captures the current composed source height and cannot undo that
      // sculpt merely because the camera is stationary during publication.
      let garden = controller.state.garden ?? HabitatGarden()
      smoothTarget = garden.surfaceHeight(
        baseHeight: world.terrain.height(point.x, point.z), at: .init(x: point.x, z: point.z))
    } else { smoothTarget = nil }
    return .init(
      action: action, location: location, radius: radius, strength: strength,
      terrainTargetHeight: smoothTarget, origin: origin, direction: direction, reach: maxReach,
      sourceRevisions: .init(garden: controller.state.garden?.revision ?? 0,
        construction: controller.state.buildings.revision,
        ecology: controller.state.population.ecology.revision), preparedTarget: target)
  }

  /// Requeries the original production ray and verifies every mutable source
  /// which can create a water or structural obstruction after preparation.
  public func validateNatureIntent(_ intent: SanctuaryNatureIntent) -> SanctuaryNatureIntent.Validation {
    let state = controller.state
    guard (state.garden?.revision ?? 0) == intent.sourceRevisions.garden else {
      return .init(command: nil, target: intent.preparedTarget, rejection: .staleGarden)
    }
    guard state.buildings.revision == intent.sourceRevisions.construction else {
      return .init(command: nil, target: intent.preparedTarget, rejection: .staleConstruction)
    }
    guard state.population.ecology.revision == intent.sourceRevisions.ecology else {
      return .init(command: nil, target: intent.preparedTarget, rejection: .staleEcology)
    }
    let target = target(origin: intent.origin, direction: intent.direction, maxReach: intent.reach)
    guard target.obstruction != .rangeLimit && target.obstruction != .outsideWorld else {
      return .init(command: nil, target: target, rejection: .outOfReach)
    }
    if target.hitKind == .walkableConstruction || target.hitKind == .construction {
      return .init(command: nil, target: target, rejection: .structure)
    }
    guard target.obstruction == .none else {
      return .init(command: nil, target: target, rejection: .occluded)
    }
    guard target.hitKind == .ground, let support = target.support else {
      return .init(command: nil, target: target, rejection: .unsupportedTarget)
    }
    let delta = SIMD2<Float>(support.point.x - intent.location.x, support.point.z - intent.location.z)
    guard length(delta).isFinite, length(delta) <= 0.02 else {
      return .init(command: nil, target: target, rejection: .occluded)
    }
    // The host may still be publishing matching terrain/water support to its
    // regional collision source. Every brush except reeds must respect the
    // saved shallow-water candidate now, rather than accepting a duplicate
    // water or dry-ground brush during the publication delay.
    if let water = localWaterHeight(support.point.x, support.point.z, state: state),
      water > support.point.y + 0.01 {
      // Reeds are the deliberate wet-bank companion to invited shallow water.
      // The direct production controls already allow this overlap; preserve that
      // authored ecology through the ordinary transient brush as well.
      if case .plant(.reeds) = intent.action {
        return .init(command: intent.command, target: target, rejection: nil)
      }
      return .init(command: nil, target: target, rejection: .water)
    }
    return .init(command: intent.command, target: target, rejection: nil)
  }

  /// Commits a previously prepared brush through one controller transaction.
  /// Rejections and failed persistence leave the garden and the transient intent unchanged.
  @discardableResult
  public func castNatureIntent(_ intent: SanctuaryNatureIntent) throws -> HabitatGarden.PatchID {
    let validation = validateNatureIntent(intent)
    guard let command = validation.command, validation.rejection == nil else {
      throw SanctuaryNatureIntentError.rejected(validation.rejection ?? .unsupportedTarget)
    }
    return try commitNature(command, expectedGardenRevision: intent.sourceRevisions.garden,
      expectedConstructionRevision: intent.sourceRevisions.construction,
      expectedEcologyRevision: intent.sourceRevisions.ecology)
  }
}
