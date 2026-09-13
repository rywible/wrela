import Foundation
import simd

extension SanctuaryWorld {
  /// Returns no more than the nearby actor bodies that can overlap the largest
  /// valid construction footprint. Heights come from the same elevation source
  /// used by wildlife support and player perception, including hover clearance.
  public func placementOccupants(
    near location: PersonalConstruction.Location
  ) -> [SanctuaryPlacementIntent.Context.Occupant] {
    placementOccupants(in: controller.state.population, near: location)
  }

  /// This accepts the transaction candidate's population as well as the live
  /// one above. `commitPlacement` calls it from `editLiving`, keeping the
  /// final actor overlap check in the same candidate that is validated and
  /// saved with the construction edit.
  func placementOccupants(
    in population: WildlifePopulation, near location: PersonalConstruction.Location
  ) -> [SanctuaryPlacementIntent.Context.Occupant] {
    guard location.x.isFinite, location.y.isFinite, location.z.isFinite else { return [] }
    let target = SIMD2<Float>(location.x, location.z)
    return population.actors.compactMap { actor in
      let distanceToTarget = distance(actor.position, target)
      guard distanceToTarget <= 12 else { return nil }
      let elevation = elevation(for: actor)
      let focusOffset = actor.species.sanctuaryFocusOffset * actor.morphologyScale
      let radius = min(1.25, max(0.28, focusOffset * 0.55 + 0.05))
      let top = elevation.focusY + max(0.18, focusOffset * 0.35)
      return SanctuaryPlacementIntent.Context.Occupant(
        id: actor.id, center: actor.position, radius: radius, bottom: elevation.rootY, top: top)
    }
    .sorted {
      let left = distance_squared($0.center, target), right = distance_squared($1.center, target)
      return left == right ? $0.id < $1.id : left < right
    }
    .prefix(25)
    .map { $0 }
  }

  /// The authoritative water source used by both transient previews and the
  /// save-backed construction transaction. Targeting deliberately reports the
  /// terrain bed under water, so bridge/deck drafts are lifted to this surface.
  public func constructionWaterSurfaceHeight(at location: PersonalConstruction.Location) -> Float? {
    guard location.x.isFinite, location.y.isFinite, location.z.isFinite else { return nil }
    return localWaterHeight(location.x, location.z, state: controller.state)
  }

  /// Resolves the only water-supported player construction to the authoritative
  /// shallow-water surface. Other primitives keep the terrain-bed target and
  /// are rejected by the common policy below when that target is wet.
  public func constructionPlacementLocation(
    for primitive: PersonalConstruction.Primitive, at target: PersonalConstruction.Location
  ) -> PersonalConstruction.Location {
    guard primitive.supportsShallowWater,
      let water = constructionWaterSurfaceHeight(at: target), water > target.y + 0.01
    else { return target }
    return .init(x: target.x, y: water, z: target.z)
  }

  /// Returns a player-created placement that the ordinary construction selector
  /// can reach. This is a read-only selection for transient native editing.
  public func placementSelectionCandidate() -> PersonalConstruction.Placement? {
    let tools = controller.state.craftTools
    let direction = SIMD2(sin(yaw), -cos(yaw))
    return nearestPlayerConstruction(
      in: controller.state.buildings, player: SIMD2(position.x, position.z), forward: direction,
      reach: tools.reach)
  }

  /// Commits a preview through the same revision-checked construction model and
  /// player-safety rule used by production building controls. Preview geometry
  /// is never committed separately from this controller transaction.
  @discardableResult
  public func commitPlacement(
    _ command: PersonalConstruction.Command, expectedRevision: UInt64
  ) throws -> PersonalConstruction.PlacementID {
    syncExpeditionPlayer()
    let placementID = try controller.editLiving { state -> PersonalConstruction.PlacementID in
      let prepared = try waterValidatedCandidate(
        command, expectedRevision: expectedRevision, in: state)
      let candidate = prepared.construction
      let placementID = prepared.placementID
      let eye = state.travel.mode == .riding ? Float(2.5) : Float(1.72)
      let feet = (state.playerElevation ?? position.y) - eye
      guard !candidate.blocksStanding(
        at: .init(x: state.player.x, y: feet, z: state.player.y), eyeHeight: eye)
      else { throw PersonalConstructionError.wouldTrapPlayer }
      // A removal only creates clearance. Every command that leaves an
      // affected placement present (including undo restoring one) must avoid
      // the actor positions in this save candidate.
      if let placement = candidate.placement(id: placementID) {
        let occupants = placementOccupants(in: state.population, near: placement.location)
        let affectedFacts = candidate.collisionFacts.filter { $0.placementID == placementID }
        guard !affectedFacts.contains(where: { fact in
          occupants.contains { SanctuaryPlacementIntent.overlaps(fact, occupant: $0) }
        }) else {
          throw SanctuaryPlacementIntentError.rejected(.occupiedAnimal)
        }
      }
      state.construction = candidate
      state.playerElevation = resolvedEyeHeight(state: state)
      return placementID
    }
    regroundPlayer()
    return placementID
  }

  /// Compatibility actions use the same water policy and atomic persistence
  /// as preview confirmation, while retaining their established scripted
  /// selection/occupancy behavior for CPU journey fixtures.
  @discardableResult
  func commitLegacyPlacement(
    _ command: PersonalConstruction.Command, expectedRevision: UInt64
  ) throws -> PersonalConstruction.PlacementID {
    syncExpeditionPlayer()
    let placementID = try controller.editLiving { state -> PersonalConstruction.PlacementID in
      let prepared = try waterValidatedCandidate(
        command, expectedRevision: expectedRevision, in: state)
      let eye = state.travel.mode == .riding ? Float(2.5) : Float(1.72)
      let feet = (state.playerElevation ?? position.y) - eye
      guard !prepared.construction.blocksStanding(
        at: .init(x: state.player.x, y: feet, z: state.player.y), eyeHeight: eye)
      else { throw PersonalConstructionError.wouldTrapPlayer }
      state.construction = prepared.construction
      state.playerElevation = resolvedEyeHeight(state: state)
      return prepared.placementID
    }
    regroundPlayer()
    return placementID
  }

  private func waterValidatedCandidate(
    _ command: PersonalConstruction.Command, expectedRevision: UInt64, in state: Expedition
  ) throws -> (construction: PersonalConstruction, placementID: PersonalConstruction.PlacementID) {
    var candidate = state.buildings
    let resolvedCommand = resolvedWaterSupport(command, in: state)
    let placementID = try candidate.apply(resolvedCommand, expectedRevision: expectedRevision)
    if let placement = candidate.placement(id: placementID),
      constructionIsWet(placement, in: state), !placement.primitive.supportsShallowWater {
      throw SanctuaryPlacementIntentError.rejected(.waterRequiresWalkway)
    }
    return (candidate, placementID)
  }

  private func resolvedWaterSupport(
    _ command: PersonalConstruction.Command, in state: Expedition
  ) -> PersonalConstruction.Command {
    switch command {
    case let .place(primitive, location, yawRadians, scale):
      return .place(
        primitive, at: resolvedWaterSupportLocation(for: primitive, target: location, state: state),
        yawRadians: yawRadians, scale: scale)
    case let .update(id, location, yawRadians, scale):
      guard let primitive = state.buildings.placement(id: id)?.primitive else { return command }
      return .update(
        id, at: resolvedWaterSupportLocation(for: primitive, target: location, state: state),
        yawRadians: yawRadians, scale: scale)
    default:
      return command
    }
  }

  private func resolvedWaterSupportLocation(
    for primitive: PersonalConstruction.Primitive, target: PersonalConstruction.Location, state: Expedition
  ) -> PersonalConstruction.Location {
    guard primitive.supportsShallowWater,
      let water = localWaterHeight(target.x, target.z, state: state), water > target.y + 0.01
    else { return target }
    return .init(x: target.x, y: water, z: target.z)
  }

  private func constructionIsWet(_ placement: PersonalConstruction.Placement, in state: Expedition) -> Bool {
    guard let water = localWaterHeight(placement.location.x, placement.location.z, state: state) else {
      return false
    }
    return water > placement.location.y + 0.01
  }
}
