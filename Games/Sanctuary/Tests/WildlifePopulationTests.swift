import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class WildlifePopulationTests: XCTestCase {
  private let openTerrain: (SIMD2<Float>, SIMD2<Float>) -> Bool = { _, _ in true }

  private func earnTrust(_ id: String, in population: inout WildlifePopulation) throws {
    // These tests exercise established traversal behavior. RelationshipActivityTests owns the
    // production invitation, completion, cooldown, refusal, and save/restore progression paths.
    population = try SanctuaryRelationshipFixture.established(population, id: id)
  }

  func testInitialPopulationIsStableBoundedAndCoversEveryBiome() throws {
    let first = WildlifePopulation.initial(seed: 41)
    let second = WildlifePopulation.initial(seed: 41)
    XCTAssertEqual(first, second)
    XCTAssertLessThanOrEqual(first.actors.count, WildlifePopulation.maximumActors)
    XCTAssertEqual(Set(first.actors.map(\.biome)), Set(SanctuaryBiome.allCases))
    XCTAssertEqual(Set(first.actors.map(\.id)).count, first.actors.count)
    XCTAssertEqual(first.actor(id: "frostling-001")?.position, Expedition.den)
    XCTAssertEqual(first.actor(id: "sunhare-001")?.position, SIMD2<Float>(2, 18))
    XCTAssertTrue(first.actor(id: "cloud-ray-001")?.capabilities.contains(.fly) == true)
    XCTAssertTrue(first.actor(id: "cloudstepper-001")?.capabilities.contains(.ride) == true)
    XCTAssertTrue(first.actor(id: "moonhart-001")?.capabilities.contains(.moveBoulders) == true)
    XCTAssertTrue(first.actor(id: "cloudstepper-001")?.capabilities.contains(.moveBoulders) == true)
    XCTAssertTrue(first.actor(id: "saltback-001")?.capabilities.contains(.moveBoulders) == true)
    XCTAssertFalse(first.actor(id: "frostling-001")?.capabilities.contains(.moveBoulders) == true)
    try first.validate()
  }

  func testGoldenMeadowFamilyStartsOnAuthoredDryBank() throws {
    let population = WildlifePopulation.initial(seed: 17)
    let terrain = Terrain()
    let adult = try XCTUnwrap(population.actor(id: "sunhare-002"))
    let young = try XCTUnwrap(population.actor(id: "sunhare-young-001"))
    let landmark = try XCTUnwrap(
      SanctuaryGeography.landmarks.first { $0.id == "golden-meadow" })

    XCTAssertNil(terrain.resolvedWaterField(at: adult.nativeHome))
    XCTAssertNil(terrain.resolvedWaterField(at: young.nativeHome))
    XCTAssertLessThan(distance(adult.nativeHome, landmark.coordinate), 40)
    XCTAssertLessThan(distance(young.nativeHome, landmark.coordinate), 40)
    XCTAssertEqual(young.parentID, adult.id)
    XCTAssertEqual(young.familyID, adult.familyID)
  }

  func testPopulationSaveAndFutureAreExactAndDecodeRejectsMissingBiome() throws {
    var garden = HabitatGarden()
    _ = try garden.apply(
      .plant(.flowers, at: .init(x: 4, z: 18), radius: 2),
      expectedRevision: garden.revision)
    var original = WildlifePopulation.initial(seed: 77)
    try original.advance(
      seconds: 0.37, player: SIMD2(2, 19), running: false, garden: garden,
      canTraverse: openTerrain, isVisible: { $0.id == "sunhare-001" })
    var restored = try JSONDecoder().decode(
      WildlifePopulation.self, from: JSONEncoder().encode(original))
    XCTAssertEqual(restored, original)
    for step in 0..<24 {
      let running = step.isMultiple(of: 7)
      try original.advance(
        seconds: 0.13, player: SIMD2(2 + Float(step) * 0.02, 19), running: running,
        garden: garden, canTraverse: openTerrain, isVisible: { $0.id == "sunhare-001" })
      try restored.advance(
        seconds: 0.13, player: SIMD2(2 + Float(step) * 0.02, 19), running: running,
        garden: garden, canTraverse: openTerrain, isVisible: { $0.id == "sunhare-001" })
      XCTAssertEqual(restored, original)
    }

    var object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(original))
      as! [String: Any]
    var actors = object["actors"] as! [[String: Any]]
    actors.removeAll { $0["biome"] as? String == SanctuaryBiome.ocean.rawValue }
    object["actors"] = actors
    XCTAssertThrowsError(
      try JSONDecoder().decode(
        WildlifePopulation.self, from: JSONSerialization.data(withJSONObject: object)))
  }

  func testNearbyActorsUseFullBrainDistantActorsUseCoarseUpdatesAndNoOfflineTimePasses()
    throws
  {
    var population = WildlifePopulation.initial(seed: 17)
    let garden = HabitatGarden()
    let nearID = "sunhare-001"
    let farID = "cloud-ray-001"
    let player = try XCTUnwrap(population.actor(id: nearID)?.position) + SIMD2<Float>(1, 0)
    try population.advance(
      seconds: 1, player: player, running: false, garden: garden,
      canTraverse: openTerrain, isVisible: { $0.id == nearID })
    let near = try XCTUnwrap(population.actor(id: nearID))
    let far = try XCTUnwrap(population.actor(id: farID))
    XCTAssertEqual(near.simulation.tick, 60)
    XCTAssertGreaterThan(far.simulation.tick, 0)
    XCTAssertLessThanOrEqual(far.simulation.tick, 2)
    XCTAssertGreaterThan(near.relationship.trust, 0)
    XCTAssertEqual(far.relationship.trust, 0)

    let saved = try JSONEncoder().encode(population)
    let reopened = try JSONDecoder().decode(WildlifePopulation.self, from: saved)
    XCTAssertEqual(reopened, population)
  }

  func testDistantCoarseBrainIsIndependentOfLocalCollisionResidency() throws {
    var immediate = WildlifePopulation.initial(seed: 17)
    var hostCommitted = immediate
    let nearbyID = "sunhare-002"
    let distantID = "brookweaver-001"
    let player = try XCTUnwrap(immediate.actor(id: nearbyID)?.position)
    let distantInitial = try XCTUnwrap(immediate.actor(id: distantID)?.simulation)

    for _ in 0..<60 {
      try immediate.advance(
        seconds: 1, player: player, running: false, garden: HabitatGarden(),
        canTraverse: { _, _ in true }, isVisible: { _ in false })
      try hostCommitted.advance(
        seconds: 1, player: player, running: false, garden: HabitatGarden(),
        // This models the native host's local committed window: its closure can
        // reject a point for lack of publication, even though the distant
        // authored habitat is not an obstacle in the wildlife simulation.
        canTraverse: { _, _ in false }, isVisible: { _ in false })
    }

    let immediateDistant = try XCTUnwrap(immediate.actor(id: distantID))
    let committedDistant = try XCTUnwrap(hostCommitted.actor(id: distantID))
    XCTAssertEqual(committedDistant, immediateDistant,
      "Remote persistent wildlife state must not depend on renderer residency")
    XCTAssertNotEqual(immediateDistant.simulation.seed, distantInitial.seed,
      "The regression must exercise real coarse brain choices, not a dormant actor")

    XCTAssertNotEqual(
      hostCommitted.actor(id: nearbyID)?.simulation,
      immediate.actor(id: nearbyID)?.simulation,
      "A nearby actor must continue to honor the detailed production traversal closure")
    XCTAssertEqual(hostCommitted.tick, immediate.tick)
    try hostCommitted.validate()
    try immediate.validate()
  }

  func testAddressValidationIsAtomicAndNearestSelectionUsesVisibleActor() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let sunhare = try XCTUnwrap(population.actor(id: "sunhare-001"))
    let player = sunhare.position + SIMD2<Float>(1, 0)

    let before = population
    for unsupported in ["invent a story", "Play with me, but leave me alone."] {
      let beforeRejection = population
      XCTAssertThrowsError(
        try population.address(
          unsupported, player: player, expectedRevision: population.revision,
          isVisible: { _ in true })
      ) { error in
        XCTAssertEqual(error as? WildlifePopulationError, .unrecognizedRequest)
        XCTAssertEqual(
          error.localizedDescription,
          "Try hello, play with me, come here, follow me, or wait here.")
      }
      XCTAssertEqual(population, beforeRejection)
    }
    XCTAssertEqual(population, before)

    XCTAssertThrowsError(
      try population.address(
        .greeting, targetID: "absent", player: player,
        expectedRevision: population.revision, isVisible: { _ in true }))
    XCTAssertEqual(population, before)

    XCTAssertThrowsError(
      try population.address(
        .greeting, targetID: sunhare.id, player: player,
        expectedRevision: population.revision + 1, isVisible: { _ in true }))
    XCTAssertEqual(population, before)

    XCTAssertThrowsError(
      try population.address(
        .greeting, targetID: sunhare.id, player: player,
        expectedRevision: population.revision, isVisible: { _ in false }))
    XCTAssertEqual(population, before)

    XCTAssertThrowsError(
      try population.address(
        .greeting, targetID: "cloud-ray-001", player: player,
        expectedRevision: population.revision, isVisible: { _ in true }))
    XCTAssertEqual(population, before)

    let result = try population.address(
      "hello", player: player, expectedRevision: population.revision,
      isVisible: { $0.id == sunhare.id })
    XCTAssertEqual(result.animalID, sunhare.id)
    XCTAssertTrue(result.accepted)
    XCTAssertEqual(result.signal, .greeting)
    XCTAssertGreaterThan(result.revision, 0)
    XCTAssertNotEqual(population, before)
  }

  func testOpeningCameraCanGreetPromptedSunhareWithinSharedRequestRange() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let openingPlayer = SIMD2<Float>(0, 24)
    let sunhare = try XCTUnwrap(population.actor(id: "sunhare-001"))
    let openingDistance = distance(openingPlayer, sunhare.position)
    XCTAssertGreaterThan(openingDistance, 6)
    XCTAssertLessThanOrEqual(openingDistance, WildlifePopulation.requestRange)

    let greeting = try population.address(
      "hello", targetID: sunhare.id, player: openingPlayer,
      expectedRevision: population.revision, isVisible: { $0.id == sunhare.id })
    XCTAssertTrue(greeting.accepted)
    XCTAssertEqual(greeting.animalID, sunhare.id)
    XCTAssertEqual(greeting.signal, .greeting)

    let beforeRejection = population
    let beyondRange = sunhare.position + SIMD2<Float>(WildlifePopulation.requestRange + 0.01, 0)
    XCTAssertThrowsError(
      try population.address(
        .greeting, targetID: sunhare.id, player: beyondRange,
        expectedRevision: population.revision, isVisible: { $0.id == sunhare.id })
    ) { error in
      XCTAssertEqual(error as? WildlifePopulationError, .tooFar)
    }
    XCTAssertEqual(population, beforeRejection)
  }

  func testAdvertisedPlayInvitationAppliesButContradictoryVersionIsAtomic() throws {
    let id = "sunhare-001"
    var population = try SanctuaryRelationshipFixture.established(
      WildlifePopulation.initial(seed: 17), id: id)
    let player = try XCTUnwrap(population.actor(id: id)?.position) + SIMD2<Float>(1, 0)
    let result = try population.address(
      "play with me", targetID: id, player: player, expectedRevision: population.revision,
      isVisible: { $0.id == id })
    XCTAssertEqual(result.request, .play)
    XCTAssertTrue(result.accepted)

    let beforeContradiction = population
    XCTAssertThrowsError(
      try population.address(
        "Play with me, but leave me alone.", targetID: id, player: player,
        expectedRevision: population.revision, isVisible: { $0.id == id })
    ) { error in
      XCTAssertEqual(error as? WildlifePopulationError, .unrecognizedRequest)
    }
    XCTAssertEqual(population, beforeContradiction)
  }

  func testSpacedAcceptedInteractionsEnablePlayFollowWaitAndTrustNeverDecays() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "sunhare-001"
    try earnTrust(id, in: &population)
    let player = try XCTUnwrap(population.actor(id: id)?.position) + SIMD2<Float>(1, 0)
    let earned = try XCTUnwrap(population.actor(id: id)?.relationship.trust)
    XCTAssertEqual(earned, 1)

    var result = try population.address(
      .play, targetID: id, player: player, expectedRevision: population.revision,
      isVisible: { $0.id == id })
    XCTAssertTrue(result.accepted)
    XCTAssertEqual(result.signal, .playing)
    result = try population.address(
      .wait, targetID: id, player: player, expectedRevision: population.revision,
      isVisible: { $0.id == id })
    XCTAssertTrue(result.accepted)
    XCTAssertEqual(result.signal, .waiting)
    result = try population.address(
      .follow, targetID: id, player: player, expectedRevision: population.revision,
      isVisible: { $0.id == id })
    XCTAssertTrue(result.accepted)
    XCTAssertEqual(result.signal, .following)

    try population.advance(
      seconds: 1, player: SIMD2(-4_000, 4_000), running: true, garden: HabitatGarden(),
      canTraverse: openTerrain, isVisible: { _ in false })
    XCTAssertEqual(population.actor(id: id)?.relationship.trust, earned)
  }

  func testRefusalIsRememberedAsNoncombatPosture() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "dunefox-001"
    let player = try XCTUnwrap(population.actor(id: id)?.position) + SIMD2<Float>(1, 0)
    let result = try population.address(
      .follow, targetID: id, player: player, expectedRevision: population.revision,
      isVisible: { $0.id == id })
    XCTAssertFalse(result.accepted)
    XCTAssertEqual(result.signal, .refusing)
    let actor = try XCTUnwrap(population.actor(id: id))
    XCTAssertTrue(actor.refusal)
    XCTAssertEqual(actor.simulation.state, "declining")
    XCTAssertEqual(actor.relationship.encounters.last?.outcome, .refused)
    XCTAssertFalse(actor.simulation.state.contains("attack"))
  }

  func testHabitatEditCausesBoundedRecoverableRelocationAndPersistentWork() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "sunhare-001"
    let home = try XCTUnwrap(population.actor(id: id)?.nativeHome)
    var garden = HabitatGarden()
    let water = try garden.apply(
      .plant(.shallowWater, at: .init(x: home.x, z: home.y), radius: 2),
      expectedRevision: garden.revision)
    try population.advance(
      seconds: 1 / 60, player: home + SIMD2(20, 0), running: false, garden: garden,
      canTraverse: openTerrain, isVisible: { _ in false })
    let displaced = try XCTUnwrap(population.actor(id: id))
    XCTAssertNotNil(displaced.relocationTarget)
    XCTAssertLessThanOrEqual(
      distance(try XCTUnwrap(displaced.relocationTarget), displaced.simulation.home), 6.01)
    XCTAssertEqual(
      population.observation(for: id, player: home, visible: false)?.signal, .seekingHabitat)
    XCTAssertGreaterThan(displaced.habitatWork.progress, 0)

    _ = try garden.apply(.restore(water), expectedRevision: garden.revision)
    try population.advance(
      seconds: 1 / 60, player: home + SIMD2(20, 0), running: false, garden: garden,
      canTraverse: openTerrain, isVisible: { _ in false })
    XCTAssertNil(population.actor(id: id)?.relocationTarget)
  }

  func testMigrationReplacesExactLegacyIdentityWithoutDuplication() throws {
    var relationship = AnimalRelationship.baked(
      id: "frostling-001", seed: 17, companionWilling: true)
    relationship.observeCalmPresence(seconds: 2)
    var creature = CreatureSimulation(home: Expedition.den, seed: 91)
    var stimulus = CreatureStimulus()
    stimulus.food = nil
    creature.step(stimulus)
    let population = try WildlifePopulation.migrating(
      seed: 44, relationship: relationship, creature: creature)
    XCTAssertEqual(population.actors.filter { $0.id == "frostling-001" }.count, 1)
    XCTAssertEqual(population.actor(id: "frostling-001")?.relationship, relationship)
    XCTAssertEqual(population.actor(id: "frostling-001")?.simulation, creature)
  }

  func testLocalTerrainEditRelocatesWithoutDeletingActorAndRestoreCancelsPlan() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "sunhare-001"
    let home = try XCTUnwrap(population.actor(id: id)?.nativeHome)
    var garden = HabitatGarden()
    let terrainPatch = try garden.apply(
      .sculpt(
        .raise, at: .init(x: home.x, z: home.y), radius: 3, amount: 1,
        targetHeight: nil), expectedRevision: garden.revision)
    try population.advance(
      seconds: 1 / 60, player: home + SIMD2(20, 0), running: false, garden: garden,
      canTraverse: openTerrain, isVisible: { _ in false })
    XCTAssertNotNil(population.actor(id: id)?.relocationTarget)
    XCTAssertNotNil(population.actor(id: id))

    _ = try garden.apply(.restore(terrainPatch), expectedRevision: garden.revision)
    try population.advance(
      seconds: 1 / 60, player: home + SIMD2(20, 0), running: false, garden: garden,
      canTraverse: openTerrain, isVisible: { _ in false })
    XCTAssertNil(population.actor(id: id)?.relocationTarget)
  }

  func testTrustedSpecificSpeciesCanRideFlyAndSyncWithinWorld() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "cloud-ray-001"
    try earnTrust(id, in: &population)
    let actor = try XCTUnwrap(population.actor(id: id))
    XCTAssertTrue(actor.companion.rideEligible)
    XCTAssertTrue(actor.companion.flyEligible)
    let destination = actor.position + SIMD2<Float>(1, 0)
    try population.updateCompanionPosition(
      id: id, position: destination, using: .fly, expectedRevision: population.revision,
      canTraverse: openTerrain)
    XCTAssertEqual(population.actor(id: id)?.position, destination)
    let actorTick = try XCTUnwrap(population.actor(id: id)?.simulation.tick)
    try population.advance(
      seconds: 1, player: destination, running: true, garden: HabitatGarden(), mountedID: id,
      canTraverse: openTerrain, isVisible: { $0.id == id })
    XCTAssertEqual(population.actor(id: id)?.simulation.tick, actorTick)
  }

  func testFollowingReanchorsAcrossRegionsWithoutTeleporting() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "sunhare-001"
    try earnTrust(id, in: &population)
    let start = try XCTUnwrap(population.actor(id: id)?.position)
    let destination = start + SIMD2<Float>(30, 0)
    _ = try population.address(
      .follow, targetID: id, player: start + SIMD2<Float>(1, 0),
      expectedRevision: population.revision, isVisible: { $0.id == id })
    var previous = start
    for _ in 0..<24 {
      try population.advance(
        seconds: 1, player: destination, running: false, garden: HabitatGarden(),
        canTraverse: openTerrain, isVisible: { $0.id == id })
      let current = try XCTUnwrap(population.actor(id: id)?.position)
      XCTAssertLessThanOrEqual(distance(current, previous), CreatureMotion().stride * 2)
      previous = current
    }
    let follower = try XCTUnwrap(population.actor(id: id))
    XCTAssertGreaterThan(distance(follower.position, start), 10)
    XCTAssertGreaterThan(distance(follower.simulation.home, start), 10)
    XCTAssertEqual(follower.simulation.activeRequest, .follow)
  }

  func testRegionalGroupsAndEarlyRideCandidateAreReachableFacts() throws {
    let population = WildlifePopulation.initial(seed: 17)
    XCTAssertGreaterThan(population.actors.count, SanctuaryBiome.allCases.count)
    XCTAssertGreaterThanOrEqual(population.actors.filter { $0.species == .sunhare }.count, 3)
    XCTAssertGreaterThanOrEqual(population.actors.filter { $0.species == .reedwalker }.count, 4)
    XCTAssertEqual(population.actors.filter { $0.species == .cloudRay }.count, 1)
    let earlyRide = try XCTUnwrap(population.actor(id: "moonhart-001"))
    XCTAssertTrue(earlyRide.capabilities.contains(.ride))
    XCTAssertLessThan(
      distance(earlyRide.position, SanctuaryGeography().landmark(for: .woodland).coordinate), 150)
    let lakeShore = try XCTUnwrap(population.actor(id: "moonhart-002"))
    XCTAssertGreaterThan(
      length((lakeShore.position - SanctuaryGeography().landmark(for: .lake).coordinate)
        / SIMD2<Float>(1_050, 820)), 1)
  }

  func testCompletedHabitatWorkCreatesDamFactsAndSameStructureRebuildsAfterEdit() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let ownerID = "brookweaver-001"
    let player = try XCTUnwrap(population.actor(id: ownerID)?.position) + SIMD2<Float>(20, 0)
    var garden = HabitatGarden()
    for _ in 0..<181 {
      try population.advance(
        seconds: 1, player: player, running: false, garden: garden,
        canTraverse: openTerrain, isVisible: { _ in false })
    }
    let built = try XCTUnwrap(population.ecology.structure(ownerID: ownerID))
    XCTAssertEqual(built.kind, .dam)
    XCTAssertEqual(built.state, .active)
    XCTAssertEqual(built.progress, 1)
    XCTAssertNotNil(population.ecology.collisionFacts.first { $0.structureID == built.id })
    XCTAssertEqual(
      population.ecology.waterFacts.first { $0.structureID == built.id }?.waterLevelRise, 0.28)
    let builtRevision = built.revision
    XCTAssertEqual(
      try JSONDecoder().decode(
        WildlifePopulation.self, from: JSONEncoder().encode(population)),
      population)

    let edit = try garden.apply(
      .plant(
        .grove, at: .init(x: built.position.x, z: built.position.y),
        radius: 1), expectedRevision: garden.revision)
    try population.advance(
      seconds: 1 / 60, player: player, running: false, garden: garden,
      canTraverse: openTerrain, isVisible: { _ in false })
    XCTAssertEqual(population.ecology.structure(id: built.id)?.state, .displaced)
    XCTAssertNil(population.ecology.collisionFacts.first { $0.structureID == built.id })
    XCTAssertNil(population.ecology.waterFacts.first { $0.structureID == built.id })

    _ = try garden.apply(.restore(edit), expectedRevision: garden.revision)
    try population.advance(
      seconds: 1 / 60, player: player, running: false, garden: garden,
      canTraverse: openTerrain, isVisible: { _ in false })
    XCTAssertEqual(population.ecology.structure(id: built.id)?.state, .rebuilding)
    for _ in 0..<181 {
      try population.advance(
        seconds: 1, player: player, running: false, garden: garden,
        canTraverse: openTerrain, isVisible: { _ in false })
    }
    let rebuilt = try XCTUnwrap(population.ecology.structure(ownerID: ownerID))
    XCTAssertEqual(rebuilt.id, built.id)
    XCTAssertEqual(rebuilt.state, .active)
    XCTAssertGreaterThan(rebuilt.revision, builtRevision)
    XCTAssertNotNil(population.ecology.collisionFacts.first { $0.structureID == built.id })
  }

  func testAuthoredYoungGrowsOnlyThroughPopulationTicksAndKeepsIdentity() throws {
    var population = WildlifePopulation.initial(seed: 17)
    let id = "sunhare-young-001"
    let initial = try XCTUnwrap(population.actor(id: id))
    XCTAssertEqual(initial.parentID, "sunhare-002")
    XCTAssertEqual(initial.familyID, "golden-sunhare-family")
    XCTAssertEqual(initial.lifeStage, .young)
    XCTAssertEqual(initial.morphologyScale, 0.62, accuracy: 0.0001)

    try population.advance(
      seconds: 1, player: SIMD2<Float>(0, 18), running: false, garden: HabitatGarden(),
      canTraverse: openTerrain, isVisible: { _ in false })
    let growing = try XCTUnwrap(population.actor(id: id))
    XCTAssertEqual(growing.growthTicks, 60)
    XCTAssertGreaterThan(growing.morphologyScale, initial.morphologyScale)

    var restored = try JSONDecoder().decode(
      WildlifePopulation.self, from: JSONEncoder().encode(population))
    XCTAssertEqual(restored, population)
    XCTAssertEqual(restored.actor(id: id)?.growthTicks, growing.growthTicks)
    for _ in 0..<3 {
      try population.advance(
        seconds: 0.37, player: SIMD2<Float>(0, 18), running: false,
        garden: HabitatGarden(), canTraverse: openTerrain, isVisible: { _ in false })
      try restored.advance(
        seconds: 0.37, player: SIMD2<Float>(0, 18), running: false,
        garden: HabitatGarden(), canTraverse: openTerrain, isVisible: { _ in false })
    }
    XCTAssertEqual(restored, population)

    for _ in 0..<359 {
      try population.advance(
        seconds: 1, player: SIMD2<Float>(0, 18), running: false, garden: HabitatGarden(),
        canTraverse: openTerrain, isVisible: { _ in false })
    }
    let adult = try XCTUnwrap(population.actor(id: id))
    XCTAssertEqual(adult.id, id)
    XCTAssertEqual(adult.lifeStage, .adult)
    XCTAssertEqual(adult.morphologyScale, 1, accuracy: 0.0001)
  }

  func testPopulationBeforeEcologyAndGrowthFieldsDecodesAsAdultsWithEmptyRegistry() throws {
    let current = WildlifePopulation.initial(seed: 29)
    var object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(current))
      as! [String: Any]
    object.removeValue(forKey: "ecology")
    var actors = object["actors"] as! [[String: Any]]
    for index in actors.indices {
      actors[index].removeValue(forKey: "familyID")
      actors[index].removeValue(forKey: "parentID")
      actors[index].removeValue(forKey: "maturityTicks")
      actors[index].removeValue(forKey: "growthTicks")
    }
    object["actors"] = actors
    let restored = try JSONDecoder().decode(
      WildlifePopulation.self, from: JSONSerialization.data(withJSONObject: object))
    XCTAssertTrue(restored.ecology.structures.isEmpty)
    XCTAssertTrue(restored.actors.allSatisfy { $0.lifeStage == .adult && $0.morphologyScale == 1 })
    XCTAssertEqual(restored.actors.map(\.id), current.actors.map(\.id))
    XCTAssertEqual(
      restored.actor(id: "frostling-001")?.relationship,
      current.actor(id: "frostling-001")?.relationship)
    XCTAssertEqual(
      restored.actor(id: "frostling-001")?.simulation,
      current.actor(id: "frostling-001")?.simulation)
  }

  func testWetlandPlantingAttractsFamilyThroughBrainAndRemovalReturnsThemExactly() throws {
    var garden = HabitatGarden()
    var population = WildlifePopulation.initial(seed: 17)
    let primaryID = "reedwalker-001"
    let parentID = "reedwalker-002"
    let youngID = "reedwalker-young-001"
    let origin = try XCTUnwrap(population.actor(id: primaryID)?.simulation.home)
    let destination = origin + SIMD2<Float>(24, 0)
    let patchID = try garden.apply(
      .plant(
        .reeds, at: .init(x: destination.x, z: destination.y), radius: 2),
      expectedRevision: garden.revision)
    let player = origin + SIMD2<Float>(20, 20)

    try population.advance(
      seconds: 1 / 60, player: player, running: false, garden: garden,
      canTraverse: openTerrain, isVisible: { _ in false })
    XCTAssertEqual(population.actor(id: primaryID)?.attractedPatchID, patchID)
    XCTAssertTrue(population.actor(id: primaryID)?.isHabitatMigrating == true)
    XCTAssertEqual(population.actor(id: parentID)?.attractedPatchID, patchID)
    XCTAssertEqual(population.actor(id: youngID)?.attractedPatchID, patchID)
    XCTAssertEqual(population.actor(id: youngID)?.parentID, parentID)

    for _ in 0..<10 {
      try population.advance(
        seconds: 1, player: player, running: false, garden: garden,
        canTraverse: openTerrain, isVisible: { _ in false })
    }
    XCTAssertLessThan(
      distance(try XCTUnwrap(population.actor(id: primaryID)?.position), destination),
      distance(origin, destination))
    var replay = try JSONDecoder().decode(
      WildlifePopulation.self, from: JSONEncoder().encode(population))
    for _ in 0..<55 {
      try population.advance(
        seconds: 1, player: player, running: false, garden: garden,
        canTraverse: openTerrain, isVisible: { _ in false })
      try replay.advance(
        seconds: 1, player: player, running: false, garden: garden,
        canTraverse: openTerrain, isVisible: { _ in false })
    }
    XCTAssertEqual(replay, population)
    let arrived = try XCTUnwrap(population.actor(id: primaryID))
    XCTAssertLessThan(distance(arrived.simulation.home, destination), 0.5)
    XCTAssertLessThan(distance(arrived.position, destination), 3)
    XCTAssertFalse(arrived.isHabitatMigrating)
    XCTAssertEqual(arrived.attractedPatchID, patchID)

    _ = try garden.apply(.restore(patchID), expectedRevision: garden.revision)
    try population.advance(
      seconds: 1 / 60, player: player, running: false, garden: garden,
      canTraverse: openTerrain, isVisible: { _ in false })
    XCTAssertNil(population.actor(id: primaryID)?.attractedPatchID)
    XCTAssertTrue(population.actor(id: primaryID)?.isHabitatMigrating == true)
    for _ in 0..<65 {
      try population.advance(
        seconds: 1, player: player, running: false, garden: garden,
        canTraverse: openTerrain, isVisible: { _ in false })
    }
    let returned = try XCTUnwrap(population.actor(id: primaryID))
    XCTAssertLessThan(distance(returned.simulation.home, origin), 0.5)
    XCTAssertLessThan(distance(returned.position, origin), 3)
    XCTAssertNil(returned.habitatOrigin)
    XCTAssertFalse(returned.isHabitatMigrating)
    XCTAssertNotNil(population.actor(id: youngID))
    XCTAssertEqual(population.actor(id: youngID)?.parentID, parentID)
  }

  func testLegacyWetGoldenMeadowFamilyWalksToUneditedDryBankAndRebuildsStableBed()
    throws
  {
    let adultID = "sunhare-002"
    let youngID = "sunhare-young-001"
    let terrain = Terrain()
    let meadow = try XCTUnwrap(
      SanctuaryGeography.landmarks.first { $0.id == "golden-meadow" }).coordinate
    let legacyAdultHome = meadow + SIMD2<Float>(4, -3)
    let legacyYoungHome = meadow + SIMD2<Float>(5.5, -3.5)
    XCTAssertNotNil(terrain.resolvedWaterField(at: legacyAdultHome))
    XCTAssertNotNil(terrain.resolvedWaterField(at: legacyYoungHome))

    // First preserve a completed ecology consequence and established relationship
    // from a successful prior save, then encode the exact superseded authored homes.
    var source = WildlifePopulation.initial(seed: 17)
    for _ in 0..<181 {
      try source.advance(
        seconds: 1, player: SIMD2<Float>(0, 18), running: false, garden: HabitatGarden(),
        canTraverse: openTerrain, isVisible: { _ in false })
    }
    source = try SanctuaryRelationshipFixture.established(source, id: adultID)
    let sourceAdult = try XCTUnwrap(source.actor(id: adultID))
    let rememberedRelationship = sourceAdult.relationship
    let rememberedGrowth = try XCTUnwrap(source.actor(id: youngID)?.growthTicks)
    let bed = try XCTUnwrap(source.ecology.structure(ownerID: adultID))
    XCTAssertEqual(bed.kind, .restingBed)
    XCTAssertEqual(bed.state, .active)

    var object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(source))
      as! [String: Any]
    var actors = object["actors"] as! [[String: Any]]
    for index in actors.indices {
      let id = actors[index]["id"] as? String
      guard id == adultID || id == youngID else { continue }
      let legacy = id == adultID ? legacyAdultHome : legacyYoungHome
      actors[index]["nativeHome"] = [legacy.x, legacy.y]
      var simulation = actors[index]["simulation"] as! [String: Any]
      for key in ["position", "home", "start", "target"] {
        simulation[key] = [legacy.x, legacy.y]
      }
      actors[index]["simulation"] = simulation
    }
    object["actors"] = actors
    var ecology = object["ecology"] as! [String: Any]
    var structures = ecology["structures"] as! [[String: Any]]
    let bedIndex = try XCTUnwrap(structures.firstIndex { $0["id"] as? String == bed.id })
    structures[bedIndex]["position"] = [legacyAdultHome.x, legacyAdultHome.y]
    ecology["structures"] = structures
    object["ecology"] = ecology
    var population = try JSONDecoder().decode(
      WildlifePopulation.self, from: JSONSerialization.data(withJSONObject: object))

    var garden = HabitatGarden()
    let reservedPrimaryBank = meadow + SIMD2<Float>(26, 8)
    let waterPatchID = try garden.apply(
      .plant(
        .shallowWater,
        at: .init(x: reservedPrimaryBank.x, z: reservedPrimaryBank.y), radius: 2),
      expectedRevision: garden.revision)
    let gardenBefore = garden
    let adultBefore = try XCTUnwrap(population.actor(id: adultID))
    try population.advance(
      seconds: 1 / 60, player: meadow + SIMD2<Float>(50, 50), running: false,
      garden: garden, canTraverse: openTerrain, isVisible: { _ in false })

    let migratingAdult = try XCTUnwrap(population.actor(id: adultID))
    let migratingYoung = try XCTUnwrap(population.actor(id: youngID))
    XCTAssertEqual(garden, gardenBefore)
    XCTAssertNotEqual(migratingAdult.nativeHome, reservedPrimaryBank)
    XCTAssertNil(terrain.resolvedWaterField(at: migratingAdult.nativeHome, garden: garden))
    XCTAssertNil(terrain.resolvedWaterField(at: migratingYoung.nativeHome, garden: garden))
    XCTAssertTrue(migratingAdult.isHabitatMigrating)
    XCTAssertTrue(migratingYoung.isHabitatMigrating)
    XCTAssertLessThanOrEqual(distance(migratingAdult.position, adultBefore.position), 1.1)
    XCTAssertEqual(migratingAdult.relationship, rememberedRelationship)
    XCTAssertGreaterThanOrEqual(migratingYoung.growthTicks, rememberedGrowth)
    XCTAssertLessThanOrEqual(migratingYoung.growthTicks, rememberedGrowth + 1)
    XCTAssertEqual(migratingYoung.parentID, adultID)
    XCTAssertEqual(migratingYoung.familyID, "golden-sunhare-family")
    XCTAssertEqual(population.ecology.structure(id: bed.id)?.state, .displaced)

    var replay = try JSONDecoder().decode(
      WildlifePopulation.self, from: JSONEncoder().encode(population))
    for _ in 0..<90 {
      try population.advance(
        seconds: 1, player: meadow + SIMD2<Float>(50, 50), running: false,
        garden: garden, canTraverse: openTerrain, isVisible: { _ in false })
      try replay.advance(
        seconds: 1, player: meadow + SIMD2<Float>(50, 50), running: false,
        garden: garden, canTraverse: openTerrain, isVisible: { _ in false })
    }
    XCTAssertEqual(replay, population)
    let arrivedAdult = try XCTUnwrap(population.actor(id: adultID))
    let arrivedYoung = try XCTUnwrap(population.actor(id: youngID))
    XCTAssertFalse(arrivedAdult.isHabitatMigrating)
    XCTAssertFalse(arrivedYoung.isHabitatMigrating)
    XCTAssertLessThan(distance(arrivedAdult.simulation.home, arrivedAdult.nativeHome), 0.5)
    XCTAssertLessThan(distance(arrivedYoung.simulation.home, arrivedYoung.nativeHome), 0.5)
    XCTAssertEqual(arrivedAdult.relationship, rememberedRelationship)
    XCTAssertEqual(arrivedYoung.parentID, adultID)
    XCTAssertNotNil(garden.patches.first { $0.id == waterPatchID })

    let rebuilding = try XCTUnwrap(population.ecology.structure(id: bed.id))
    XCTAssertEqual(rebuilding.id, bed.id)
    XCTAssertEqual(rebuilding.position, arrivedAdult.nativeHome)
    XCTAssertTrue(rebuilding.state == .rebuilding || rebuilding.state == .active)
    XCTAssertGreaterThan(rebuilding.revision, bed.revision)
  }
}
