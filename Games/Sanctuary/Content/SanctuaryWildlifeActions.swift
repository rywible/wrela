import FieldCore
import Foundation
import SimulationCore
import simd

extension SanctuaryWorld {
  public func animalVisible(_ actor: WildlifeActor) -> Bool {
    let target = V3(actor.position.x, elevation(for: actor).focusY, actor.position.y)
    let delta = target - position
    guard length_squared(delta) > 0.001, length_squared(delta) < 80 * 80,
      dot(normalize(delta), forward) > 0.3 else { return false }
    for i in 1..<20 {
      let point = position + delta * (Float(i) / 20)
      if point.y < groundHeight(point.x, point.z) { return false }
      for index in world.collisionGrid.candidates(at: point) where world.solids[index].value(point) < 0 {
        return false
      }
      if constructionFacts.contains(where: {
        $0.kind == .solid && $0.contains(.init(x: point.x, y: point.y, z: point.z))
      }) { return false }
    }
    return true
  }

  public var nearbyAnimal: WildlifeActor? {
    controller.state.population.nearest(to: SIMD2(position.x, position.z), within: WildlifePopulation.requestRange,
      matching: animalVisible)
  }

  public var livingObservations: [String: String] {
    let state = controller.state
    let animal = nearbyAnimal
    return [
      "nearestLandmarkID": world.terrain.geography.nearestLandmark(to: state.player).id,
      "discoveredLandmarkIDs": state.travel.discoveredLandmarks.sorted().joined(separator: ","),
      "boulderMoves": String(state.movedBoulders.history.count),
      "recordedCreatureDecisions": String(state.decisionJournal.sequence),
      "habitatMigrants": String(state.population.actors.filter { $0.isHabitatMigrating }.count),
      "populationCount": String(state.population.actors.count),
      "observedAnimals": String(state.travel.observedAnimals.count),
      "discoveredPlaces": String(state.travel.discoveredLandmarks.count),
      "gardenRevision": String(state.garden?.revision ?? 0),
      "gardenPatchCount": String(state.garden?.patches.count ?? 0),
      "terrainPatchCount": String(state.garden?.terrainPatches.count ?? 0),
      "buildingCount": String(state.buildings.placements.count),
      "travelMode": state.travel.mode.rawValue,
      "nearbyAnimalID": animal?.id ?? "",
      "populationRevision": String(state.population.revision),
      "ecologyStructureCount": String(state.population.ecology.structures.count),
      "activeEcologyStructures": String(state.population.ecology.structures.filter { $0.state == .active }.count),
      "youngAnimals": String(state.population.actors.filter { $0.lifeStage == .young }.count),
      "lastAnimalRequest": animal?.relationship.encounters.last?.request.rawValue ?? "",
      "lastAnimalResponse": animal?.relationship.encounters.last?.outcome.rawValue ?? "",
      "nearbyAnimalActivity": animal?.relationship.activeActivity?.kind.rawValue ?? "",
      "nearbyAnimalExperiences": String(animal?.relationship.sharedExperiences.count ?? 0),
      "lastAnimalReturnResponse": animal?.relationship.returnEvents.last?.response.rawValue ?? "",
      "animalReturnResponses": String(animal?.relationship.returnEvents.count ?? 0),
      "animalAssistanceOutcomes": String(animal?.relationship.assistanceMemories.count ?? 0),
      "lastAnimalAssistance": animal?.relationship.assistanceMemories.last?.kind.rawValue ?? ""
    ]
  }

  public func request(_ text: String) throws -> String {
    try SimulationSemanticInput.validateRequest(text)
    guard text.count <= 160 else { throw SimulationFailure.invalid("Try a request of 160 characters or fewer") }
    if legacyInteractions {
      syncExpeditionPlayer()
      return try controller.addressCreature(text, visible: creatureVisible)
    }
    if ["help move this boulder", "move this boulder", "please move this boulder"].contains(
      text.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()) {
      return try control("help-boulder")
    }
    guard let request = AnimalRequestParser.parse(text) else {
      throw WildlifePopulationError.unrecognizedRequest
    }
    syncExpeditionPlayer()
    let player = SIMD2(position.x, position.z)
    let result = try controller.editLiving { state in
      var population = state.population
      let result = try population.address(request, player: player,
        expectedRevision: population.revision, isVisible: animalVisible)
      state.wildlife = population
      var journey = state.travel
      journey.observe(animalID: result.animalID)
      state.journey = journey
      return result
    }
    return animalRequestFeedback(result)
  }

  private func animalRequestFeedback(_ result: WildlifeAddressResult) -> String {
    guard result.accepted else { return "The animal keeps its distance." }
    switch result.signal {
    case .greeting: return "The animal turns its attention toward you."
    case .approaching: return "The animal starts toward you."
    case .following: return "The animal moves to follow you."
    case .waiting: return "The animal settles and waits."
    case .playing: return "The animal bounds into play."
    case .refusing: return "The animal keeps its distance."
    default: return "The animal notices your invitation."
    }
  }

  /// A bounded input may move farther than one tick. Advance the attached actor
  /// along bounded segments before publishing the move, preserving replay state.
  func syncMountedCompanion() {
    guard let id = controller.state.travel.companionID else { return }
    let target = SIMD2(position.x, position.z)
    controller.simulateLiving { state in
      var population = state.population
      guard let actor = population.actor(id: id) else { return }
      let origin = actor.position
      let segments = max(1, Int(ceil(distance(origin, target) / 8)))
      do {
        for index in 1...segments {
          let point = origin + (target - origin) * (Float(index) / Float(segments))
          try population.updateCompanionPosition(id: id, position: point,
            using: state.travel.mode == .flying ? .fly : .ride,
            expectedRevision: population.revision)
        }
        state.wildlife = population
      } catch { controller.message = "Companion movement: \(error.localizedDescription)" }
    }
  }

  func advanceLiving(_ seconds: Float, running: Bool) {
    guard seconds.isFinite, seconds > 0, seconds <= 1 else { return }
    let player = SIMD2(position.x, position.z)
    // Evaluate perception before inout state access to keep Swift exclusivity clear.
    let visibleIDs = Set(controller.state.population.actors.filter(animalVisible).map(\.id))
    controller.simulateLiving { state in
      var population = state.population
      do {
        if let id = state.travel.companionID {
          try population.updateCompanionPosition(id: id, position: player,
            using: state.travel.mode == .flying ? .fly : .ride,
            expectedRevision: population.revision)
        }
        try population.advance(seconds: seconds, player: player, running: running,
          garden: state.garden ?? HabitatGarden(), mountedID: state.travel.companionID,
          actorCanTraverse: creatureCanTraverse, isVisible: { visibleIDs.contains($0.id) })
        state.wildlife = population
      } catch {
        // A rejected simulation candidate leaves the last valid population intact.
        // Exposed to native diagnostics rather than silently inventing movement.
        controller.message = "Wildlife update: \(error.localizedDescription)"
      }
      var journey = state.travel
      journey.discover(at: player)
      for actor in population.actors where visibleIDs.contains(actor.id)
        && distance(actor.position, player) <= 20 { journey.observe(animalID: actor.id) }
      state.journey = journey
    }
  }

  func interactLiving() throws -> String {
    if controller.state.travel.mode != .walking { return try companionControl("dismount") }
    if nearbyAnimal != nil { return try request("hello") }
    return try controller.editLiving { state in
      var journey = state.travel
      let clue = journey.examineClue(at: state.player)
      state.journey = journey
      return clue ?? "Pause and look around. Wildlife leaves tracks, resting places, and moving silhouettes."
    }
  }

  func companionControl(_ id: String) throws -> String {
    if id == "dismount" {
      if let water = localWaterHeight(position.x, position.z),
        water > groundHeight(position.x, position.z) + 0.5 {
        throw SimulationFailure.invalid("Your companion waits for a safe landing on shore")
      }
      try controller.editLiving { state in
        var journey = state.travel
        try journey.travel(.walking, with: nil)
        state.journey = journey
        state.playerElevation = resolvedEyeHeight(state: state)
      }
      regroundPlayer()
      return ""
    }
    if id == "higher" || id == "lowerFlight" {
      try controller.editLiving { state in
        var journey = state.travel
        guard journey.mode == .flying else { throw SimulationFailure.invalid("Find a willing flying companion first") }
        journey.flightHeight = min(60, max(3, journey.flightHeight + (id == "higher" ? 4 : -4)))
        state.journey = journey
        state.playerElevation = resolvedEyeHeight(state: state)
      }
      regroundPlayer()
      return ""
    }
    guard id == "ride" || id == "fly" else { throw SimulationFailure.invalid("Unknown companion action") }
    guard let actor = nearbyAnimal else { throw WildlifePopulationError.notVisible }
    guard id == "ride" ? actor.companion.rideEligible : actor.companion.flyEligible else {
      throw WildlifePopulationError.unavailableCapability
    }
    try controller.editLiving { state in
      var population = state.population
      try population.updateCompanionPosition(id: actor.id, position: state.player,
        using: id == "ride" ? .ride : .fly, expectedRevision: population.revision)
      state.wildlife = population
      var journey = state.travel
      try journey.travel(id == "ride" ? .riding : .flying, with: actor.id)
      state.journey = journey
      state.playerElevation = resolvedEyeHeight(state: state)
    }
    regroundPlayer()
    return ""
  }
}
