import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class RelationshipActivityTests: XCTestCase {
  private let openTerrain: (SIMD2<Float>, SIMD2<Float>) -> Bool = { _, _ in true }

  private func player(near actor: WildlifeActor, distance: Float = 6) -> SIMD2<Float> {
    actor.position + SIMD2<Float>(0, distance)
  }

  private func advance(
    _ seconds: Int, population: inout WildlifePopulation, player: SIMD2<Float>,
    visibleID: String, visible: Bool = true
  ) throws {
    for _ in 0..<seconds {
      try population.advance(
        seconds: 1, player: player, running: false, garden: HabitatGarden(),
        canTraverse: openTerrain, isVisible: { visible && $0.id == visibleID })
    }
  }

  private func blockSaves(in root: URL) throws {
    let saves = root.appendingPathComponent("saves")
    if FileManager.default.fileExists(atPath: saves.path) {
      try FileManager.default.removeItem(at: saves)
    }
    try Data("not a directory".utf8).write(to: saves)
  }

  private func establishCompanion(
    _ id: String, population: inout WildlifePopulation
  ) throws {
    var visitor = player(near: try XCTUnwrap(population.actor(id: id)))
    _ = try population.address(
      .greeting, targetID: id, player: visitor, expectedRevision: population.revision,
      isVisible: { $0.id == id })
    try advance(10, population: &population, player: visitor, visibleID: id)
    for _ in 0..<8
    where population.actor(id: id)?.relationship.isWilling(to: .follow) == false {
      visitor = try XCTUnwrap(population.actor(id: id)?.position) + SIMD2<Float>(0, 2)
      let play = try population.address(
        .play, targetID: id, player: visitor, expectedRevision: population.revision,
        isVisible: { $0.id == id })
      XCTAssertTrue(play.accepted)
      for seconds: Float in [1, 1, 0.5] {
        try population.advance(
          seconds: seconds, player: visitor, running: false, garden: HabitatGarden(),
          canTraverse: openTerrain, isVisible: { $0.id == id })
      }
    }
    XCTAssertTrue(population.actor(id: id)?.relationship.isWilling(to: .follow) == true)
  }

  func testInvitationStartsButOnlyCompletedCompanyAndPlayAwardExperience() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "sunhare-001"
    let position = player(near: try XCTUnwrap(population.actor(id: id)))

    let greeting = try population.address(
      .greeting, targetID: id, player: position, expectedRevision: population.revision,
      isVisible: { $0.id == id })
    XCTAssertTrue(greeting.accepted)
    let greetingTrust = try XCTUnwrap(population.actor(id: id)?.relationship.trust)
    XCTAssertGreaterThan(greetingTrust, 0, "A greeting adds only bounded recognition")
    XCTAssertEqual(population.actor(id: id)?.relationship.activeActivity?.kind, .company)
    XCTAssertTrue(population.actor(id: id)?.relationship.sharedExperiences.isEmpty == true)

    try advance(9, population: &population, player: position, visibleID: id)
    XCTAssertLessThanOrEqual(
      population.actor(id: id)?.relationship.trust ?? 1,
      AnimalRelationship.familiarityCap)
    XCTAssertTrue(population.actor(id: id)?.relationship.sharedExperiences.isEmpty == true)
    XCTAssertNil(population.actor(id: id)?.simulation.activeRequest,
      "Company continues after the short greeting response ends")

    let beforeCompanyCompletion = try XCTUnwrap(population.actor(id: id)?.relationship.trust)
    try advance(1, population: &population, player: position, visibleID: id)
    let afterCompany = try XCTUnwrap(population.actor(id: id))
    XCTAssertEqual(afterCompany.relationship.sharedExperiences.map(\.kind), [.company])
    XCTAssertNil(afterCompany.relationship.activeActivity)
    XCTAssertGreaterThan(afterCompany.relationship.trust, beforeCompanyCompletion)
    XCTAssertTrue(afterCompany.relationship.isWilling(to: .play),
      "One welcomed visit should lead naturally to a first play invitation")

    let beforePlay = afterCompany.relationship.trust
    let play = try population.address(
      .play, targetID: id, player: position, expectedRevision: population.revision,
      isVisible: { $0.id == id })
    XCTAssertTrue(play.accepted)
    XCTAssertEqual(population.actor(id: id)?.relationship.trust, beforePlay,
      "Accepting an invitation is not a completed shared experience")
    XCTAssertEqual(population.actor(id: id)?.relationship.sharedExperiences.count, 1)
    for _ in 0..<8 {
      let repeated = try population.address(
        .play, targetID: id, player: position, expectedRevision: population.revision,
        isVisible: { $0.id == id })
      XCTAssertTrue(repeated.accepted)
    }
    XCTAssertEqual(population.actor(id: id)?.relationship.trust, beforePlay)
    XCTAssertEqual(population.actor(id: id)?.relationship.sharedExperiences.count, 1,
      "Repeated invitations restart progress and cannot manufacture completion")

    try population.advance(
      seconds: 1, player: position, running: false, garden: HabitatGarden(),
      canTraverse: openTerrain, isVisible: { $0.id == id })
    XCTAssertEqual(population.actor(id: id)?.relationship.trust, beforePlay)
    try population.advance(
      seconds: 1, player: position, running: false, garden: HabitatGarden(),
      canTraverse: openTerrain, isVisible: { $0.id == id })
    try population.advance(
      seconds: 0.5, player: position, running: false, garden: HabitatGarden(),
      canTraverse: openTerrain, isVisible: { $0.id == id })

    let afterPlay = try XCTUnwrap(population.actor(id: id))
    XCTAssertEqual(afterPlay.relationship.sharedExperiences.map(\.kind), [.company, .play])
    XCTAssertGreaterThanOrEqual(
      afterPlay.relationship.sharedExperiences.last?.distanceTravelled ?? 0,
      AnimalRelationship.playCompletionDistance)
    XCTAssertGreaterThan(afterPlay.relationship.trust, beforePlay)
  }

  func testInvitedAnimalCrossesSavedBridgeAndOnlyCompletedCompanyBecomesMemory() throws {
    let world = try SanctuaryWorld(seed: 17)
    let center = SIMD2<Float>(100, 100)
    try world.controller.editLiving { state in
      _ = try state.applyNature(
        .plant(.shallowWater, at: .init(x: center.x, z: center.y), radius: 2))
    }
    let water = try XCTUnwrap(world.localWaterHeight(center.x, center.y))
    _ = try world.commitPlacement(
      .place(.bridge, at: .init(x: center.x, y: water, z: center.y), yawRadians: 0, scale: 1),
      expectedRevision: world.controller.state.buildings.revision)

    let id = "frostling-001"
    let start = center - SIMD2<Float>(1.3, 0)
    let visitor = center + SIMD2<Float>(1.5, 0)
    var original = try WildlifePopulation.migrating(
      seed: 17, relationship: .baked(id: id, seed: 17, trust: 1, companionWilling: true),
      creature: CreatureSimulation(home: start, seed: 41))
    let invitation = try original.address(
      .come, targetID: id, player: visitor, expectedRevision: original.revision,
      isVisible: { $0.id == id })
    XCTAssertTrue(invitation.accepted)
    XCTAssertEqual(original.actor(id: id)?.relationship.activeActivity?.kind, .company)
    XCTAssertEqual(original.actor(id: id)?.relationship.sharedExperiences, [],
      "The invitation itself is not a shared experience")

    let railStart = SIMD2<Float>(center.x - 1.3, center.y + 0.72)
    let railEnd = SIMD2<Float>(center.x + 1.5, center.y + 0.72)
    XCTAssertFalse(world.creatureCanTraverse(
      try XCTUnwrap(original.actor(id: id)), railStart, railEnd))
    let wetStart = SIMD2<Float>(center.x - 2.3, center.y + 1.1)
    let wetEnd = SIMD2<Float>(center.x + 2.3, center.y + 1.1)
    XCTAssertFalse(world.creatureCanTraverse(
      try XCTUnwrap(original.actor(id: id)), wetStart, wetEnd))

    let garden = world.controller.state.garden ?? HabitatGarden()
    for _ in 0..<300 {
      try original.advance(
        seconds: 1 / 60, player: visitor, running: false, garden: garden,
        actorCanTraverse: { actor, from, to in
          world.creatureCanTraverse(actor, from, to)
        }, isVisible: { $0.id == id })
    }
    let midpoint = try XCTUnwrap(original.actor(id: id))
    XCTAssertGreaterThan(midpoint.position.x, center.x,
      "The ordinary approach brain should cross the supported centerline")
    XCTAssertEqual(midpoint.relationship.sharedExperiences, [])
    XCTAssertEqual(midpoint.relationship.activeActivity?.activeTicks, 300)

    var replay = try JSONDecoder().decode(
      WildlifePopulation.self, from: JSONEncoder().encode(original))
    for _ in 0..<300 {
      try original.advance(
        seconds: 1 / 60, player: visitor, running: false, garden: garden,
        actorCanTraverse: { actor, from, to in
          world.creatureCanTraverse(actor, from, to)
        }, isVisible: { $0.id == id })
      try replay.advance(
        seconds: 1 / 60, player: visitor, running: false, garden: garden,
        actorCanTraverse: { actor, from, to in
          world.creatureCanTraverse(actor, from, to)
        }, isVisible: { $0.id == id })
    }
    XCTAssertEqual(replay, original)
    let completed = try XCTUnwrap(original.actor(id: id)?.relationship)
    XCTAssertNil(completed.activeActivity)
    XCTAssertEqual(completed.sharedExperiences.count, 1)
    XCTAssertEqual(completed.sharedExperiences.first?.kind, .company)
    XCTAssertEqual(completed.sharedExperiences.first?.originatingRequest, .come)
    XCTAssertEqual(completed.sharedExperiences.first?.activeTicks,
      AnimalRelationship.companyCompletionTicks)

    var refusing = try WildlifePopulation.migrating(
      seed: 17, relationship: .baked(id: id, seed: 17, trust: 0, companionWilling: true),
      creature: CreatureSimulation(home: start, seed: 41))
    let beforeRefusal = try XCTUnwrap(refusing.actor(id: id)?.relationship)
    let refusal = try refusing.address(
      .come, targetID: id, player: visitor, expectedRevision: refusing.revision,
      isVisible: { $0.id == id })
    XCTAssertFalse(refusal.accepted)
    let refused = try XCTUnwrap(refusing.actor(id: id)?.relationship)
    XCTAssertEqual(refused.trust, beforeRefusal.trust)
    XCTAssertEqual(refused.sharedExperiences, beforeRefusal.sharedExperiences)
    XCTAssertNil(refused.activeActivity)
    XCTAssertEqual(refused.encounters.last?.outcome, .refused)
    XCTAssertEqual(
      try JSONDecoder().decode(WildlifePopulation.self, from: JSONEncoder().encode(refusing)),
      refusing, "Only the truthful refused encounter/brain response persists")
  }

  func testLegacyWetActorEscapesThroughOrdinaryAcceptedApproachWithoutEarlyCredit() throws {
    let world = try SanctuaryWorld(seed: 17)
    let wetOrigin = SIMD2<Float>(110, 100)
    try world.controller.editLiving { state in
      _ = try state.applyNature(
        .plant(.shallowWater, at: .init(x: wetOrigin.x, z: wetOrigin.y), radius: 0.25))
    }
    let id = "frostling-001"
    var population = try WildlifePopulation.migrating(
      seed: 17, relationship: .baked(id: id, seed: 17, trust: 1, companionWilling: true),
      creature: CreatureSimulation(home: wetOrigin, seed: 41))
    let actor = try XCTUnwrap(population.actor(id: id))
    var openDirection: SIMD2<Float>?
    for index in 0..<16 {
      let angle: Float = Float(index) * (2 * Float.pi / 16)
      let candidate = SIMD2<Float>(cos(angle), sin(angle))
      let endpoint = wetOrigin + candidate * Float(0.72)
      if world.creatureCanTraverse(actor, wetOrigin, endpoint) {
        openDirection = candidate
        break
      }
    }
    let direction = try XCTUnwrap(openDirection,
      "No production-supported dry escape stride around the wet-origin fixture")
    let visitor = wetOrigin + direction * 2
    let response = try population.address(
      .come, targetID: id, player: visitor, expectedRevision: population.revision,
      isVisible: { $0.id == id })
    XCTAssertTrue(response.accepted)
    let garden = world.controller.state.garden ?? HabitatGarden()
    for _ in 0..<60 {
      try population.advance(
        seconds: 1 / 60, player: visitor, running: false, garden: garden,
        actorCanTraverse: { actor, from, to in
          world.creatureCanTraverse(actor, from, to)
        }, isVisible: { $0.id == id })
    }
    let escaped = try XCTUnwrap(population.actor(id: id))
    XCTAssertGreaterThan(distance(escaped.position, wetOrigin), 0.25)
    XCTAssertGreaterThan(dot(escaped.position - wetOrigin, direction), 0.25)
    XCTAssertNil(world.localWaterHeight(escaped.position.x, escaped.position.y))
    XCTAssertEqual(escaped.relationship.sharedExperiences, [])
    XCTAssertEqual(escaped.relationship.activeActivity?.kind, .company)
  }

  func testProductionRequestsReportObservedRefusalAndAcceptedResponses() throws {
    let world = try SanctuaryWorld(seed: 17)
    world.camera.position = SIMD3<Float>(
      0, world.groundHeight(0, 24) + 1.72, 24)
    world.camera.yaw = 0
    world.camera.pitch = -0.08
    world.syncExpeditionPlayer()

    XCTAssertEqual(try world.request("play with me"), "The animal keeps its distance.")
    var sunhare = try XCTUnwrap(world.controller.state.population.actor(id: "sunhare-001"))
    XCTAssertEqual(sunhare.relationship.encounters.last?.outcome, .refused)
    XCTAssertTrue(sunhare.relationship.sharedExperiences.isEmpty)

    XCTAssertEqual(
      try world.request("hello"), "The animal turns its attention toward you.")
    sunhare = try XCTUnwrap(world.controller.state.population.actor(id: "sunhare-001"))
    XCTAssertEqual(sunhare.relationship.activeActivity?.kind, .company)
    XCTAssertTrue(sunhare.relationship.sharedExperiences.isEmpty,
      "Immediate feedback observes the response; it does not claim completed company")

    for _ in 0..<AnimalRelationship.companyCompletionTicks {
      world.advance(1 / 60, running: false)
    }
    XCTAssertEqual(
      world.controller.state.population.actor(id: "sunhare-001")?
        .relationship.sharedExperiences.map(\.kind), [.company])
    XCTAssertEqual(try world.request("play with me"), "The animal bounds into play.")
    sunhare = try XCTUnwrap(world.controller.state.population.actor(id: "sunhare-001"))
    XCTAssertEqual(sunhare.relationship.activeActivity?.kind, .play)
    XCTAssertEqual(sunhare.relationship.sharedExperiences.map(\.kind), [.company],
      "Accepted play feedback must not claim completion credit")
  }

  func testFailedRequestCommitReturnsNoSuccessFeedbackAndPreservesState() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let world = try SanctuaryWorld(root: root, seed: 17)
    world.camera.position = SIMD3<Float>(
      0, world.groundHeight(0, 24) + 1.72, 24)
    world.camera.yaw = 0
    world.camera.pitch = -0.08
    world.syncExpeditionPlayer()
    try blockSaves(in: root)
    let before = try world.checkpoint()

    XCTAssertThrowsError(try world.request("hello"))
    XCTAssertEqual(try world.checkpoint(), before)
    XCTAssertTrue(
      world.controller.state.population.actor(id: "sunhare-001")?
        .relationship.encounters.isEmpty == true)
  }

  func testMidpointSaveRestoresExactActivityFutureAndLegacySaveDefaults() throws {
    var original = WildlifePopulation.initial(seed: 17)
    let id = "sunhare-001"
    let position = player(near: try XCTUnwrap(original.actor(id: id)))
    _ = try original.address(
      .greeting, targetID: id, player: position, expectedRevision: original.revision,
      isVisible: { $0.id == id })
    try advance(5, population: &original, player: position, visibleID: id)
    XCTAssertEqual(
      original.actor(id: id)?.relationship.activeActivity?.activeTicks,
      AnimalRelationship.companyCompletionTicks / 2)

    var restored = try JSONDecoder().decode(
      WildlifePopulation.self, from: JSONEncoder().encode(original))
    XCTAssertEqual(restored, original)
    try advance(5, population: &original, player: position, visibleID: id)
    try advance(5, population: &restored, player: position, visibleID: id)
    XCTAssertEqual(restored, original)
    XCTAssertEqual(original.actor(id: id)?.relationship.sharedExperiences.map(\.kind), [.company])

    let relationship = AnimalRelationship.baked(id: "old-save-001", seed: 17, trust: 0.4)
    var object = try XCTUnwrap(
      JSONSerialization.jsonObject(with: JSONEncoder().encode(relationship)) as? [String: Any])
    object.removeValue(forKey: "activeActivity")
    object.removeValue(forKey: "sharedExperiences")
    object.removeValue(forKey: "awaySinceTick")
    object.removeValue(forKey: "returnEvents")
    object.removeValue(forKey: "mountedExplorationActive")
    object.removeValue(forKey: "assistanceMemories")
    let legacy = try JSONDecoder().decode(
      AnimalRelationship.self, from: JSONSerialization.data(withJSONObject: object))
    XCTAssertNil(legacy.activeActivity)
    XCTAssertEqual(legacy.sharedExperiences, [])
    XCTAssertNil(legacy.awaySinceTick)
    XCTAssertEqual(legacy.returnEvents, [])
    XCTAssertFalse(legacy.mountedExplorationActive)
    XCTAssertEqual(legacy.assistanceMemories, [])
    XCTAssertEqual(legacy.trust, relationship.trust)
    XCTAssertEqual(legacy.preferences, relationship.preferences)
  }

  func testRefusalInterruptionAndSpatialRejectionGiveNoCompletionCredit() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "sunhare-001"
    let position = player(near: try XCTUnwrap(population.actor(id: id)), distance: 7)
    _ = try population.address(
      .greeting, targetID: id, player: position, expectedRevision: population.revision,
      isVisible: { $0.id == id })
    try population.advance(
      seconds: 1, player: position, running: false, garden: HabitatGarden(),
      canTraverse: openTerrain, isVisible: { $0.id == id })

    let beforeRejection = population
    XCTAssertThrowsError(
      try population.address(
        .play, targetID: id, player: position + SIMD2<Float>(20, 0),
        expectedRevision: population.revision, isVisible: { $0.id == id }))
    XCTAssertEqual(population, beforeRejection)

    let trustBeforeRefusal = try XCTUnwrap(population.actor(id: id)?.relationship.trust)
    let refusal = try population.address(
      .follow, targetID: id, player: position, expectedRevision: population.revision,
      isVisible: { $0.id == id })
    XCTAssertFalse(refusal.accepted)
    XCTAssertNil(population.actor(id: id)?.relationship.activeActivity)
    try advance(10, population: &population, player: position, visibleID: id)
    let interrupted = try XCTUnwrap(population.actor(id: id))
    XCTAssertEqual(interrupted.relationship.sharedExperiences, [])
    XCTAssertGreaterThanOrEqual(interrupted.relationship.trust, trustBeforeRefusal)
    XCTAssertLessThanOrEqual(interrupted.relationship.trust, AnimalRelationship.familiarityCap)
    XCTAssertEqual(interrupted.relationship.encounters.last?.outcome, .refused)
  }

  func testExplorationRequiresSustainedFollowAndActualTravel() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "sunhare-001"
    var companionPosition = player(near: try XCTUnwrap(population.actor(id: id)))
    _ = try population.address(
      .greeting, targetID: id, player: companionPosition,
      expectedRevision: population.revision, isVisible: { $0.id == id })
    try advance(10, population: &population, player: companionPosition, visibleID: id)

    for _ in 0..<6 where population.actor(id: id)?.relationship.isWilling(to: .follow) == false {
      companionPosition = try XCTUnwrap(population.actor(id: id)?.position) + SIMD2<Float>(0, 2)
      let play = try population.address(
        .play, targetID: id, player: companionPosition,
        expectedRevision: population.revision, isVisible: { $0.id == id })
      XCTAssertTrue(play.accepted)
      try population.advance(
        seconds: 1, player: companionPosition, running: false, garden: HabitatGarden(),
        canTraverse: openTerrain, isVisible: { $0.id == id })
      try population.advance(
        seconds: 1, player: companionPosition, running: false, garden: HabitatGarden(),
        canTraverse: openTerrain, isVisible: { $0.id == id })
      try population.advance(
        seconds: 0.5, player: companionPosition, running: false, garden: HabitatGarden(),
        canTraverse: openTerrain, isVisible: { $0.id == id })
    }
    XCTAssertTrue(population.actor(id: id)?.relationship.isWilling(to: .follow) == true)

    companionPosition = try XCTUnwrap(population.actor(id: id)?.position) + SIMD2<Float>(1, 0)
    let beforeExploration = try XCTUnwrap(population.actor(id: id)?.relationship.trust)
    let follow = try population.address(
      .follow, targetID: id, player: companionPosition,
      expectedRevision: population.revision, isVisible: { $0.id == id })
    XCTAssertTrue(follow.accepted)
    XCTAssertEqual(population.actor(id: id)?.relationship.trust, beforeExploration)

    for _ in 0..<30 {
      companionPosition.x += 0.45
      try population.advance(
        seconds: 1, player: companionPosition, running: false, garden: HabitatGarden(),
        canTraverse: openTerrain, isVisible: { $0.id == id })
      if population.actor(id: id)?.relationship.sharedExperiences.last?.kind == .exploration {
        break
      }
    }
    let explored = try XCTUnwrap(population.actor(id: id))
    XCTAssertEqual(explored.relationship.sharedExperiences.last?.kind, .exploration)
    XCTAssertGreaterThanOrEqual(
      explored.relationship.sharedExperiences.last?.distanceTravelled ?? 0,
      AnimalRelationship.explorationCompletionDistance)
    XCTAssertGreaterThan(explored.relationship.trust, beforeExploration)
  }

  func testFamiliarPreferredPlayAnimalApproachesOnRealReturnAndReplayIsExact() throws {
    var original = WildlifePopulation.initial(seed: 17)
    let id = "frostling-001"
    var visitor = player(near: try XCTUnwrap(original.actor(id: id)))
    _ = try original.address(
      .greeting, targetID: id, player: visitor, expectedRevision: original.revision,
      isVisible: { $0.id == id })
    try advance(10, population: &original, player: visitor, visibleID: id)
    visitor = try XCTUnwrap(original.actor(id: id)?.position) + SIMD2<Float>(0, 2)
    let play = try original.address(
      .play, targetID: id, player: visitor, expectedRevision: original.revision,
      isVisible: { $0.id == id })
    XCTAssertTrue(play.accepted)
    for seconds: Float in [1, 1, 0.5] {
      try original.advance(
        seconds: seconds, player: visitor, running: false, garden: HabitatGarden(),
        canTraverse: openTerrain, isVisible: { $0.id == id })
    }
    let familiar = try XCTUnwrap(original.actor(id: id))
    XCTAssertEqual(familiar.relationship.sharedExperiences.map(\.kind), [.company, .play])

    let away = familiar.position + SIMD2<Float>(0, AnimalRelationship.returnDepartureDistance + 6)
    try advance(5, population: &original, player: away, visibleID: id, visible: false)
    XCTAssertNotNil(original.actor(id: id)?.relationship.awaySinceTick)
    var restored = try JSONDecoder().decode(
      WildlifePopulation.self, from: JSONEncoder().encode(original))
    try advance(5, population: &original, player: away, visibleID: id, visible: false)
    try advance(5, population: &restored, player: away, visibleID: id, visible: false)
    XCTAssertEqual(restored, original)

    visitor = try XCTUnwrap(original.actor(id: id)?.position) + SIMD2<Float>(0, 6)
    let trustBeforeReturn = try XCTUnwrap(original.actor(id: id)?.relationship.trust)
    try original.advance(
      seconds: 1 / 60, player: visitor, running: false, garden: HabitatGarden(),
      canTraverse: openTerrain, isVisible: { $0.id == id })
    try restored.advance(
      seconds: 1 / 60, player: visitor, running: false, garden: HabitatGarden(),
      canTraverse: openTerrain, isVisible: { $0.id == id })
    XCTAssertEqual(restored, original)
    let returned = try XCTUnwrap(original.actor(id: id))
    XCTAssertEqual(returned.relationship.returnEvents.last?.response, .approached)
    XCTAssertEqual(returned.simulation.activeRequest, .come)
    XCTAssertEqual(returned.relationship.trust, trustBeforeReturn)
    XCTAssertNil(returned.relationship.activeActivity,
      "A spontaneous response is not a player invitation or earned activity")

    let beforeApproach = distance(returned.position, visitor)
    for _ in 0..<2 {
      try original.advance(
        seconds: 1, player: visitor, running: false, garden: HabitatGarden(),
        canTraverse: openTerrain, isVisible: { $0.id == id })
    }
    XCTAssertLessThan(distance(try XCTUnwrap(original.actor(id: id)?.position), visitor), beforeApproach)
  }

  func testIndependentAndRefusingAnimalsDoNotGetForcedIntoReturnApproach() throws {
    var independent = WildlifePopulation.initial(seed: 17)
    let independentID = "sunhare-001"
    let home = try XCTUnwrap(independent.actor(id: independentID)?.position)
    let away = home + SIMD2<Float>(0, AnimalRelationship.returnDepartureDistance + 6)
    try advance(10, population: &independent, player: away, visibleID: independentID, visible: false)
    let returnedPlayer = home + SIMD2<Float>(0, 6)
    try independent.advance(
      seconds: 1 / 60, player: returnedPlayer, running: false, garden: HabitatGarden(),
      canTraverse: openTerrain, isVisible: { $0.id == independentID })
    XCTAssertEqual(independent.actor(id: independentID)?.relationship.returnEvents, [])
    XCTAssertNil(independent.actor(id: independentID)?.simulation.activeRequest)

    var refusing = WildlifePopulation.initial(seed: 17)
    let id = "frostling-001"
    let familiarPlayer = player(near: try XCTUnwrap(refusing.actor(id: id)))
    _ = try refusing.address(
      .greeting, targetID: id, player: familiarPlayer, expectedRevision: refusing.revision,
      isVisible: { $0.id == id })
    try advance(10, population: &refusing, player: familiarPlayer, visibleID: id)
    let refusal = try refusing.address(
      .follow, targetID: id, player: familiarPlayer, expectedRevision: refusing.revision,
      isVisible: { $0.id == id })
    XCTAssertFalse(refusal.accepted)
    let refusalTrust = try XCTUnwrap(refusing.actor(id: id)?.relationship.trust)
    let far = try XCTUnwrap(refusing.actor(id: id)?.position)
      + SIMD2<Float>(0, AnimalRelationship.returnDepartureDistance + 6)
    try advance(10, population: &refusing, player: far, visibleID: id, visible: false)
    let near = try XCTUnwrap(refusing.actor(id: id)?.position) + SIMD2<Float>(0, 6)
    try refusing.advance(
      seconds: 1 / 60, player: near, running: false, garden: HabitatGarden(),
      canTraverse: openTerrain, isVisible: { $0.id == id })
    XCTAssertEqual(refusing.actor(id: id)?.relationship.returnEvents, [])
    XCTAssertEqual(refusing.actor(id: id)?.relationship.trust, refusalTrust)
    XCTAssertNil(refusing.actor(id: id)?.simulation.activeRequest)
  }

  func testReturnResponseCooldownPreventsRepeatedAcknowledgement() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "sunhare-001"
    var visitor = player(near: try XCTUnwrap(population.actor(id: id)))
    _ = try population.address(
      .greeting, targetID: id, player: visitor, expectedRevision: population.revision,
      isVisible: { $0.id == id })
    try advance(10, population: &population, player: visitor, visibleID: id)
    var away = try XCTUnwrap(population.actor(id: id)?.position)
      + SIMD2<Float>(0, AnimalRelationship.returnDepartureDistance + 6)
    try advance(10, population: &population, player: away, visibleID: id, visible: false)
    visitor = try XCTUnwrap(population.actor(id: id)?.position) + SIMD2<Float>(0, 6)
    try population.advance(
      seconds: 1 / 60, player: visitor, running: false, garden: HabitatGarden(),
      canTraverse: openTerrain, isVisible: { $0.id == id })
    XCTAssertEqual(population.actor(id: id)?.relationship.returnEvents.last?.response, .acknowledged)

    away = try XCTUnwrap(population.actor(id: id)?.position)
      + SIMD2<Float>(0, AnimalRelationship.returnDepartureDistance + 6)
    try advance(10, population: &population, player: away, visibleID: id, visible: false)
    visitor = try XCTUnwrap(population.actor(id: id)?.position) + SIMD2<Float>(0, 6)
    try population.advance(
      seconds: 1 / 60, player: visitor, running: false, garden: HabitatGarden(),
      canTraverse: openTerrain, isVisible: { $0.id == id })
    XCTAssertEqual(population.actor(id: id)?.relationship.returnEvents.count, 1)
    XCTAssertNil(population.actor(id: id)?.relationship.awaySinceTick)
  }

  func testValidatedRideAndFlightCompleteExplorationAcrossMidpointSave() throws {
    for (id, capability): (String, WildlifeCapability) in [
      ("moonhart-001", .ride), ("cloud-ray-001", .fly),
    ] {
      var original = WildlifePopulation.initial(seed: 17)
      try establishCompanion(id, population: &original)
      var point = try XCTUnwrap(original.actor(id: id)?.position)

      // The first mounted synchronization establishes position but cannot earn
      // teleport distance. Six real seconds and later segments form the midpoint.
      for _ in 0..<6 {
        point += SIMD2<Float>(0.75, 0)
        try original.updateCompanionPosition(
          id: id, position: point, using: capability, expectedRevision: original.revision,
          canTraverse: openTerrain)
        try original.advance(
          seconds: 1, player: point, running: false, garden: HabitatGarden(), mountedID: id,
          canTraverse: openTerrain, isVisible: { $0.id == id })
      }
      let midpoint = try XCTUnwrap(original.actor(id: id)?.relationship.activeActivity)
      XCTAssertEqual(midpoint.kind, .exploration)
      XCTAssertEqual(midpoint.activeTicks, 360)
      XCTAssertEqual(midpoint.distanceTravelled, 3.75, accuracy: 0.001)
      var restored = try JSONDecoder().decode(
        WildlifePopulation.self, from: JSONEncoder().encode(original))

      for _ in 0..<6 {
        point += SIMD2<Float>(0.75, 0)
        try original.updateCompanionPosition(
          id: id, position: point, using: capability, expectedRevision: original.revision,
          canTraverse: openTerrain)
        try restored.updateCompanionPosition(
          id: id, position: point, using: capability, expectedRevision: restored.revision,
          canTraverse: openTerrain)
        try original.advance(
          seconds: 1, player: point, running: false, garden: HabitatGarden(), mountedID: id,
          canTraverse: openTerrain, isVisible: { $0.id == id })
        try restored.advance(
          seconds: 1, player: point, running: false, garden: HabitatGarden(), mountedID: id,
          canTraverse: openTerrain, isVisible: { $0.id == id })
      }
      XCTAssertEqual(restored, original, id)
      let completed = try XCTUnwrap(original.actor(id: id)?.relationship.sharedExperiences.last)
      XCTAssertEqual(completed.kind, .exploration)
      XCTAssertGreaterThanOrEqual(
        completed.distanceTravelled, AnimalRelationship.explorationCompletionDistance)
      XCTAssertTrue(original.actor(id: id)?.relationship.mountedExplorationActive == true,
        "Completion remains tied to this mount session and does not restart repeatedly")
    }
  }

  func testMountedTravelSignalUsesOnlyAcceptedPostAttachDistanceAndPersists() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "moonhart-001"
    try establishCompanion(id, population: &population)
    let start = try XCTUnwrap(population.actor(id: id)?.position)

    let attachment = start + SIMD2<Float>(1, 0)
    try population.updateCompanionPosition(
      id: id, position: attachment, using: .ride, expectedRevision: population.revision,
      canTraverse: openTerrain)
    let attached = try XCTUnwrap(population.actor(id: id)?.mountedTravel)
    XCTAssertTrue(attached.active)
    XCTAssertFalse(attached.isMoving, "First attachment establishes a baseline, not gait travel")
    XCTAssertEqual(attached.cycleDistance, 0)

    try population.advance(
      seconds: 1 / 60, player: attachment, running: false, garden: HabitatGarden(), mountedID: id,
      canTraverse: openTerrain, isVisible: { $0.id == id })
    let travelled = attachment + SIMD2<Float>(0.6, 0.8)
    try population.updateCompanionPosition(
      id: id, position: travelled, using: .ride, expectedRevision: population.revision,
      canTraverse: openTerrain)
    let moving = try XCTUnwrap(population.actor(id: id)?.mountedTravel)
    XCTAssertTrue(moving.isMoving)
    XCTAssertEqual(moving.latestSegmentDistance, 1, accuracy: 0.0001)
    XCTAssertEqual(moving.cycleDistance, 1, accuracy: 0.0001)
    XCTAssertEqual(moving.heading, atan2(Float(0.6), Float(-0.8)), accuracy: 0.0001)

    let encoded = try JSONEncoder().encode(population)
    var restored = try JSONDecoder().decode(WildlifePopulation.self, from: encoded)
    XCTAssertEqual(restored, population)
    try restored.advance(
      seconds: 1 / 60, player: travelled, running: false, garden: HabitatGarden(), mountedID: id,
      canTraverse: openTerrain, isVisible: { $0.id == id })
    XCTAssertTrue(restored.actor(id: id)?.mountedTravel.isMoving == true)
    try restored.advance(
      seconds: 1 / 60, player: travelled, running: false, garden: HabitatGarden(), mountedID: id,
      canTraverse: openTerrain, isVisible: { $0.id == id })
    XCTAssertFalse(restored.actor(id: id)?.mountedTravel.isMoving == true,
      "The first tick without another accepted move stops gait")

    let beforeBlocked = restored
    XCTAssertThrowsError(
      try restored.updateCompanionPosition(
        id: id, position: travelled + SIMD2<Float>(1, 0), using: .ride,
        expectedRevision: restored.revision, canTraverse: { _, _ in false }))
    XCTAssertEqual(restored, beforeBlocked)

    try restored.updateCompanionPosition(
      id: id, position: travelled + SIMD2<Float>(1, 0), using: .ride,
      expectedRevision: restored.revision, canTraverse: openTerrain)
    XCTAssertTrue(restored.actor(id: id)?.mountedTravel.isMoving == true)
    try restored.advance(
      seconds: 1 / 60, player: travelled + SIMD2<Float>(100, 0), running: false,
      garden: HabitatGarden(), canTraverse: openTerrain, isVisible: { _ in false })
    XCTAssertFalse(restored.actor(id: id)?.mountedTravel.active == true)
    XCTAssertFalse(restored.actor(id: id)?.mountedTravel.isMoving == true)
  }

  func testLegacyPopulationDefaultsMountedTravelSignalToInactive() throws {
    let population = WildlifePopulation.initial(seed: 17)
    var object = try XCTUnwrap(
      JSONSerialization.jsonObject(with: JSONEncoder().encode(population)) as? [String: Any])
    var actors = try XCTUnwrap(object["actors"] as? [[String: Any]])
    for index in actors.indices { actors[index].removeValue(forKey: "mountedTravel") }
    object["actors"] = actors
    let legacy = try JSONDecoder().decode(
      WildlifePopulation.self, from: JSONSerialization.data(withJSONObject: object))
    XCTAssertTrue(legacy.actors.allSatisfy {
      !$0.mountedTravel.active && !$0.mountedTravel.isMoving && $0.mountedTravel.cycleDistance == 0
    })
  }

  func testDismountAndDistantUpdateDiscardIncompleteMountedExploration() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "moonhart-001"
    try establishCompanion(id, population: &population)
    let priorExplorations = population.actor(id: id)?.relationship.sharedExperiences
      .filter { $0.kind == .exploration }.count ?? 0
    var point = try XCTUnwrap(population.actor(id: id)?.position)
    for _ in 0..<3 {
      point += SIMD2<Float>(0.75, 0)
      try population.updateCompanionPosition(
        id: id, position: point, using: .ride, expectedRevision: population.revision,
        canTraverse: openTerrain)
      try population.advance(
        seconds: 1, player: point, running: false, garden: HabitatGarden(), mountedID: id,
        canTraverse: openTerrain, isVisible: { $0.id == id })
    }
    XCTAssertNotNil(population.actor(id: id)?.relationship.activeActivity)

    let distant = point + SIMD2<Float>(100, 0)
    try population.advance(
      seconds: 1 / 60, player: distant, running: false, garden: HabitatGarden(),
      canTraverse: openTerrain, isVisible: { _ in false })
    let dismounted = try XCTUnwrap(population.actor(id: id))
    XCTAssertNil(dismounted.relationship.activeActivity)
    XCTAssertFalse(dismounted.relationship.mountedExplorationActive)
    XCTAssertEqual(
      dismounted.relationship.sharedExperiences.filter { $0.kind == .exploration }.count,
      priorExplorations)
  }

  func testAssistanceHookRequiresEligibleActualOutcomeAndRejectsDuplicatesAtomically() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let helperID = "moonhart-001"
    try establishCompanion(helperID, population: &population)
    let helper = try XCTUnwrap(population.actor(id: helperID))
    XCTAssertTrue(helper.companion.helpEligible)
    XCTAssertTrue(helper.capabilities.contains(.moveBoulders))
    let trustBefore = helper.relationship.trust

    try population.recordAssistance(
      animalID: helperID, outcomeID: "boulder:granite-041:push-1",
      kind: .boulderMovement, expectedRevision: population.revision)
    let recorded = try XCTUnwrap(population.actor(id: helperID))
    XCTAssertEqual(recorded.relationship.assistanceMemories.last?.kind, .boulderMovement)
    XCTAssertEqual(
      recorded.relationship.assistanceMemories.last?.outcomeID, "boulder:granite-041:push-1")
    XCTAssertEqual(
      recorded.relationship.trust,
      min(1, trustBefore + 0.1 + helper.relationship.preferences.sociability * 0.04),
      accuracy: 0.0001)

    let beforeDuplicate = population
    XCTAssertThrowsError(
      try population.recordAssistance(
        animalID: helperID, outcomeID: "boulder:granite-041:push-1",
        kind: .boulderMovement, expectedRevision: population.revision)) { error in
      XCTAssertEqual(error as? WildlifePopulationError, .duplicateAssistanceOutcome)
    }
    XCTAssertEqual(population, beforeDuplicate)

    XCTAssertThrowsError(
      try population.recordAssistance(
        animalID: "sunhare-001", outcomeID: "habitat:patch-2:restored",
        kind: .habitatRestoration, expectedRevision: population.revision)) { error in
      XCTAssertEqual(error as? WildlifePopulationError, .invalidAssistanceOutcome)
    }
    XCTAssertEqual(population, beforeDuplicate)

    var restored = try JSONDecoder().decode(
      WildlifePopulation.self, from: JSONEncoder().encode(population))
    XCTAssertEqual(restored, population)
    try restored.recordAssistance(
      animalID: helperID, outcomeID: "habitat:willow-bank:restored",
      kind: .habitatRestoration, expectedRevision: restored.revision)
    XCTAssertEqual(restored.actor(id: helperID)?.relationship.assistanceMemories.count, 2)
    for index in 0..<(AnimalRelationship.assistanceMemoryLimit + 2) {
      try restored.recordAssistance(
        animalID: helperID, outcomeID: "habitat:restoration:\(index)",
        kind: .habitatRestoration, expectedRevision: restored.revision)
    }
    XCTAssertEqual(
      restored.actor(id: helperID)?.relationship.assistanceMemories.count,
      AnimalRelationship.assistanceMemoryLimit)
    try restored.validate()
  }

  func testCompletedExperienceMemoryIsBoundedAndPreferenceChangesCredit() throws {
    func companyResult(for id: String) throws -> AnimalRelationship {
      var population = WildlifePopulation.initial(seed: 17)
      let position = player(near: try XCTUnwrap(population.actor(id: id)))
      _ = try population.address(
        .greeting, targetID: id, player: position, expectedRevision: population.revision,
        isVisible: { $0.id == id })
      try advance(10, population: &population, player: position, visibleID: id)
      return try XCTUnwrap(population.actor(id: id)?.relationship)
    }

    let sunhare = try companyResult(for: "sunhare-001")
    let frostling = try companyResult(for: "frostling-001")
    XCTAssertNotEqual(sunhare.preferences.sociability, frostling.preferences.sociability)
    XCTAssertNotEqual(sunhare.trust, frostling.trust,
      "Authored sociability and favorite requests affect completed company credit")

    var population = WildlifePopulation.initial(seed: 17)
    let id = "sunhare-001"
    let position = player(near: try XCTUnwrap(population.actor(id: id)))
    for _ in 0..<(AnimalRelationship.sharedExperienceLimit + 2) {
      _ = try population.address(
        .greeting, targetID: id, player: position, expectedRevision: population.revision,
        isVisible: { $0.id == id })
      try advance(10, population: &population, player: position, visibleID: id)
    }
    XCTAssertEqual(
      population.actor(id: id)?.relationship.sharedExperiences.count,
      AnimalRelationship.sharedExperienceLimit)
    try population.validate()
  }

  func testProductionWorldCompletesOpeningCompanyAndPlayAcrossReopen() throws {
    let saveRoot = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: saveRoot) }
    let world = try SanctuaryWorld(root: saveRoot, seed: 17)
    world.camera.position = SIMD3<Float>(
      0, world.groundHeight(0, 24) + 1.72, 24)
    world.camera.yaw = 0
    world.camera.pitch = -0.08
    world.syncExpeditionPlayer()

    _ = try world.request("hello")
    XCTAssertEqual(world.controller.state.population.actor(id: "sunhare-001")?
      .relationship.activeActivity?.kind, .company)
    for _ in 0..<(AnimalRelationship.companyCompletionTicks / 2) {
      world.advance(1 / 60, running: false)
    }
    let midpoint = try world.checkpoint()
    try world.controller.save()
    let reopened = try SanctuaryWorld(root: saveRoot, seed: 17)
    XCTAssertEqual(reopened.controller.state.population, world.controller.state.population)

    for _ in 0..<(AnimalRelationship.companyCompletionTicks / 2) {
      world.advance(1 / 60, running: false)
      reopened.advance(1 / 60, running: false)
    }
    XCTAssertEqual(reopened.controller.state.population, world.controller.state.population)
    let welcomed = try XCTUnwrap(
      reopened.controller.state.population.actor(id: "sunhare-001"))
    XCTAssertEqual(welcomed.relationship.sharedExperiences.map(\.kind), [.company])
    XCTAssertTrue(welcomed.relationship.isWilling(to: .play))

    let beforeInvitation = welcomed.relationship.trust
    _ = try reopened.request("play")
    XCTAssertEqual(
      reopened.controller.state.population.actor(id: "sunhare-001")?.relationship.trust,
      beforeInvitation)
    for _ in 0..<AnimalRelationship.playCompletionTicks {
      reopened.advance(1 / 60, running: false)
    }
    let played = try XCTUnwrap(
      reopened.controller.state.population.actor(id: "sunhare-001"))
    XCTAssertEqual(played.relationship.sharedExperiences.map(\.kind), [.company, .play])
    XCTAssertGreaterThan(played.relationship.trust, beforeInvitation)

    // Fixture positioning isolates the return detector; all response and
    // locomotion after each position is advanced through the production world.
    let separated = played.position
      + SIMD2<Float>(0, AnimalRelationship.returnDepartureDistance + 6)
    reopened.camera.position = SIMD3(
      separated.x, reopened.groundHeight(separated.x, separated.y) + 1.72, separated.y)
    reopened.syncExpeditionPlayer()
    for _ in 0..<AnimalRelationship.returnAbsenceTicks {
      reopened.advance(1 / 60, running: false)
    }
    let returnedPoint = try XCTUnwrap(
      reopened.controller.state.population.actor(id: "sunhare-001")?.position)
      + SIMD2<Float>(0, 6)
    reopened.camera.position = SIMD3(
      returnedPoint.x, reopened.groundHeight(returnedPoint.x, returnedPoint.y) + 1.72,
      returnedPoint.y)
    reopened.camera.yaw = 0
    reopened.syncExpeditionPlayer()
    reopened.advance(1 / 60, running: false)
    let acknowledged = try XCTUnwrap(
      reopened.controller.state.population.actor(id: "sunhare-001"))
    XCTAssertEqual(acknowledged.relationship.returnEvents.last?.response, .approached)
    XCTAssertEqual(acknowledged.simulation.activeRequest, .come)

    try world.restore(midpoint)
    XCTAssertEqual(
      world.controller.state.population.actor(id: "sunhare-001")?
        .relationship.activeActivity?.activeTicks,
      AnimalRelationship.companyCompletionTicks / 2)
  }

  func testPhysicalReopenPreservesIndividualReturnAndCooldownForIndependentAndCompanion() throws {
    try verifyPhysicalReturn(
      animalID: "sunhare-002", expectedTemperament: .cautious,
      expectedResponse: .acknowledged, completePreferredPlay: false)
    try verifyPhysicalReturn(
      animalID: "sunhare-001", expectedTemperament: .proud,
      expectedResponse: .approached, completePreferredPlay: true)
  }

  private func verifyPhysicalReturn(
    animalID: String, expectedTemperament: AnimalTemperament,
    expectedResponse: AnimalReturnResponse, completePreferredPlay: Bool
  ) throws {
    let saveRoot = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: saveRoot) }
    let world = try SanctuaryWorld(root: saveRoot, seed: 17)

    func placeCamera(_ world: SanctuaryWorld, distanceFromAnimal: Float, facingAnimal: Bool) throws {
      let actor = try XCTUnwrap(world.controller.state.population.actor(id: animalID))
      let point = actor.position + SIMD2<Float>(0, distanceFromAnimal)
      world.camera.position = SIMD3(
        point.x, world.groundHeight(point.x, point.y) + 1.72, point.y)
      world.camera.yaw = facingAnimal ? 0 : .pi
      world.camera.pitch = -0.08
      world.syncExpeditionPlayer()
    }

    try placeCamera(world, distanceFromAnimal: 2, facingAnimal: true)
    let initial = try XCTUnwrap(world.controller.state.population.actor(id: animalID))
    XCTAssertEqual(initial.relationship.preferences.temperament, expectedTemperament)
    if animalID == "sunhare-002" {
      XCTAssertFalse(initial.relationship.preferences.companionWilling,
        "The distant cautious fixture remains an authored independent individual")
    } else {
      XCTAssertTrue(initial.relationship.preferences.companionWilling,
        "The cabin Sunhare is the authored welcomed companion fixture")
    }

    _ = try world.request("hello")
    XCTAssertEqual(
      world.controller.state.population.actor(id: animalID)?.relationship.encounters.last?.request,
      .greeting, "The ordinary text control must select the intended visible individual")
    for _ in 0..<AnimalRelationship.companyCompletionTicks {
      world.advance(1 / 60, running: false)
    }
    XCTAssertEqual(
      world.controller.state.population.actor(id: animalID)?.relationship.sharedExperiences
        .map(\.kind), [.company])

    if completePreferredPlay {
      try placeCamera(world, distanceFromAnimal: 2, facingAnimal: true)
      _ = try world.request("play")
      for _ in 0..<AnimalRelationship.playCompletionTicks {
        world.advance(1 / 60, running: false)
      }
      XCTAssertEqual(
        world.controller.state.population.actor(id: animalID)?.relationship.sharedExperiences
          .map(\.kind), [.company, .play])
    }

    // The first half of the real absence is committed to disk. Reconstructing
    // SanctuaryWorld exercises the ordinary ExpeditionStore reopen path rather
    // than a JSON-only relationship round trip.
    try placeCamera(
      world, distanceFromAnimal: AnimalRelationship.returnDepartureDistance + 16,
      facingAnimal: false)
    for _ in 0..<(AnimalRelationship.returnAbsenceTicks / 2) {
      world.advance(1 / 60, running: false)
    }
    let awaySince = try XCTUnwrap(
      world.controller.state.population.actor(id: animalID)?.relationship.awaySinceTick)
    try world.controller.save()
    let reopenedDuringAbsence = try SanctuaryWorld(root: saveRoot, seed: 17)
    XCTAssertEqual(
      reopenedDuringAbsence.controller.state.population.actor(id: animalID)?
        .relationship.awaySinceTick, awaySince)
    for _ in 0..<(AnimalRelationship.returnAbsenceTicks / 2) {
      reopenedDuringAbsence.advance(1 / 60, running: false)
    }

    try placeCamera(reopenedDuringAbsence, distanceFromAnimal: 2, facingAnimal: true)
    reopenedDuringAbsence.advance(1 / 60, running: false)
    let returned = try XCTUnwrap(
      reopenedDuringAbsence.controller.state.population.actor(id: animalID))
    XCTAssertEqual(returned.relationship.returnEvents.map(\.response), [expectedResponse])
    XCTAssertNil(returned.relationship.activeActivity,
      "Recognition is not a new invitation or shared-experience reward")
    XCTAssertEqual(
      returned.simulation.activeRequest,
      expectedResponse == .approached ? .come : .greeting)
    XCTAssertTrue(
      reopenedDuringAbsence.controller.state.population.actors.filter { $0.id != animalID }
        .allSatisfy { $0.relationship.returnEvents.isEmpty },
      "A return is remembered only by the individual that shared the visit")

    try reopenedDuringAbsence.controller.save()
    let reopenedDuringCooldown = try SanctuaryWorld(root: saveRoot, seed: 17)
    XCTAssertEqual(
      reopenedDuringCooldown.controller.state.population.actor(id: animalID)?
        .relationship.returnEvents, returned.relationship.returnEvents)

    try placeCamera(
      reopenedDuringCooldown,
      distanceFromAnimal: AnimalRelationship.returnDepartureDistance + 16,
      facingAnimal: false)
    for _ in 0..<AnimalRelationship.returnAbsenceTicks {
      reopenedDuringCooldown.advance(1 / 60, running: false)
    }
    try placeCamera(reopenedDuringCooldown, distanceFromAnimal: 2, facingAnimal: true)
    reopenedDuringCooldown.advance(1 / 60, running: false)
    let cooled = try XCTUnwrap(
      reopenedDuringCooldown.controller.state.population.actor(id: animalID))
    XCTAssertEqual(cooled.relationship.returnEvents.count, 1,
      "A physical reopen must not reset the bounded return-response cooldown")
    XCTAssertNil(cooled.relationship.awaySinceTick,
      "A return inside cooldown consumes its separation without queuing a later response")
  }
}
