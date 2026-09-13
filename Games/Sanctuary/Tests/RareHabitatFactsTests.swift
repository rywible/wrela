import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class RareHabitatFactsTests: XCTestCase {
  private let openTerrain: (SIMD2<Float>, SIMD2<Float>) -> Bool = { _, _ in true }

  func testFactsIncludeOnlyObservedExplicitRareIndividual() throws {
    let population = WildlifePopulation.initial(seed: 17)
    let garden = HabitatGarden()
    XCTAssertEqual(
      WildlifePopulation.discoveryCharacterizations.map(\.animalID), ["brookweaver-001"])
    XCTAssertEqual(
      population.discoveryCharacterization(for: "brookweaver-001")?.rarity, .rare)
    XCTAssertNil(population.discoveryCharacterization(for: "brookweaver-002"))
    XCTAssertTrue(
      population.rareHabitatFacts(observedAnimalIDs: [], garden: garden).isEmpty)
    XCTAssertTrue(
      population.rareHabitatFacts(
        observedAnimalIDs: ["brookweaver-002", "unknown"], garden: garden
      ).isEmpty)

    let facts = population.rareHabitatFacts(
      observedAnimalIDs: ["brookweaver-001", "brookweaver-001"], garden: garden)
    let fact = try XCTUnwrap(facts.first)
    XCTAssertEqual(facts.count, 1)
    XCTAssertEqual(fact.characterization.animalID, "brookweaver-001")
    XCTAssertEqual(fact.characterization.evidence, .call)
    XCTAssertEqual(fact.species, .brookweaver)
    XCTAssertEqual(fact.preferredPlanting, .shallowWater)
    XCTAssertEqual(fact.state, .observed)
    XCTAssertNil(fact.supportingPatchID)
    XCTAssertNil(fact.structure)
  }

  func testRareHabitatFactReplaysArrivalAndRealStructureRecovery() throws {
    let actorID = "brookweaver-001"
    var population = WildlifePopulation.initial(seed: 31)
    var garden = HabitatGarden()
    let origin = try XCTUnwrap(population.actor(id: actorID)?.simulation.home)
    let destination = origin + SIMD2<Float>(24, 0)
    let patchID = try garden.apply(
      .plant(
        .shallowWater, at: .init(x: destination.x, z: destination.y), radius: 2),
      expectedRevision: garden.revision)
    let player = destination + SIMD2<Float>(20, 20)

    try population.advance(
      seconds: 1 / 60, player: player, running: false, garden: garden,
      canTraverse: openTerrain, isVisible: { _ in false })
    var fact = try XCTUnwrap(population.rareHabitatFacts(
      observedAnimalIDs: [actorID], garden: garden).first)
    XCTAssertEqual(fact.state, .approachingPreferredHabitat)
    XCTAssertEqual(fact.supportingPatchID, patchID)
    XCTAssertNil(fact.structure)

    for _ in 0..<10 {
      try population.advance(
        seconds: 1, player: player, running: false, garden: garden,
        canTraverse: openTerrain, isVisible: { _ in false })
    }
    var replay = try JSONDecoder().decode(
      WildlifePopulation.self, from: JSONEncoder().encode(population))
    let restoredGarden = try JSONDecoder().decode(
      HabitatGarden.self, from: JSONEncoder().encode(garden))
    XCTAssertEqual(
      replay.rareHabitatFacts(observedAnimalIDs: [actorID], garden: restoredGarden),
      population.rareHabitatFacts(observedAnimalIDs: [actorID], garden: garden))

    for _ in 0..<60 {
      try population.advance(
        seconds: 1, player: player, running: false, garden: garden,
        canTraverse: openTerrain, isVisible: { _ in false })
      try replay.advance(
        seconds: 1, player: player, running: false, garden: restoredGarden,
        canTraverse: openTerrain, isVisible: { _ in false })
    }
    XCTAssertEqual(replay, population)
    fact = try XCTUnwrap(population.rareHabitatFacts(
      observedAnimalIDs: [actorID], garden: garden).first)
    XCTAssertEqual(fact.state, .settledAtPreferredHabitat)

    for _ in 0..<100 {
      try population.advance(
        seconds: 1, player: player, running: false, garden: garden,
        canTraverse: openTerrain, isVisible: { _ in false })
    }
    fact = try XCTUnwrap(population.rareHabitatFacts(
      observedAnimalIDs: [actorID], garden: garden).first)
    let active = try XCTUnwrap(fact.structure)
    XCTAssertEqual(active.ownerID, actorID)
    XCTAssertEqual(active.kind, .dam)
    XCTAssertEqual(active.state, .active)
    let activeRevision = active.revision

    let obstructionID = try garden.apply(
      .plant(
        .grove, at: .init(x: active.position.x, z: active.position.y), radius: 1),
      expectedRevision: garden.revision)
    try population.advance(
      seconds: 1 / 60, player: player, running: false, garden: garden,
      canTraverse: openTerrain, isVisible: { _ in false })
    fact = try XCTUnwrap(population.rareHabitatFacts(
      observedAnimalIDs: [actorID], garden: garden).first)
    XCTAssertEqual(fact.structure?.id, active.id)
    XCTAssertEqual(fact.structure?.state, .displaced)

    _ = try garden.apply(.restore(obstructionID), expectedRevision: garden.revision)
    try population.advance(
      seconds: 1 / 60, player: player, running: false, garden: garden,
      canTraverse: openTerrain, isVisible: { _ in false })
    fact = try XCTUnwrap(population.rareHabitatFacts(
      observedAnimalIDs: [actorID], garden: garden).first)
    XCTAssertEqual(fact.structure?.id, active.id)
    XCTAssertEqual(fact.structure?.state, .rebuilding)

    for _ in 0..<100 {
      try population.advance(
        seconds: 1, player: player, running: false, garden: garden,
        canTraverse: openTerrain, isVisible: { _ in false })
    }
    fact = try XCTUnwrap(population.rareHabitatFacts(
      observedAnimalIDs: [actorID], garden: garden).first)
    XCTAssertEqual(fact.structure?.id, active.id)
    XCTAssertEqual(fact.structure?.state, .active)
    XCTAssertGreaterThan(fact.structure?.revision ?? 0, activeRevision)
  }

  func testWorldControlsDriveRareHabitatAndDamRecoveryOnProductionTraversal() throws {
    let actorID = "brookweaver-001"
    let world = try SanctuaryWorld(seed: 43)
    let authoredActor = try XCTUnwrap(world.controller.state.population.actor(id: actorID))

    // Fixture setup only: place the camera three metres from the authored actor
    // and choose a short direction that the real streamed-world traversal accepts.
    // Planting, movement, construction, disruption and undo below use ordinary
    // SanctuaryWorld controls and its production simulation path.
    var chosen: (player: SIMD2<Float>, target: SIMD2<Float>, yaw: Float)?
    for index in 0..<16 {
      let angle = Float(index) * 2 * .pi / 16
      let direction = SIMD2<Float>(cos(angle), sin(angle))
      let player = authoredActor.position - direction * 3
      let yaw = atan2(direction.x, -direction.y)
      world.camera.position = SIMD3<Float>(
        player.x, world.groundHeight(player.x, player.y) + 1.72, player.y)
      world.camera.yaw = yaw
      world.camera.pitch = -0.08
      world.move(.zero)
      let resolvedPlayer = SIMD2<Float>(world.camera.position.x, world.camera.position.z)
      let target = resolvedPlayer + direction * world.controller.state.craftTools.reach
      if distance(authoredActor.position, target) <= 9.5,
        world.creatureCanTraverse(authoredActor.position, target),
        world.animalVisible(authoredActor)
      {
        chosen = (resolvedPlayer, target, yaw)
        break
      }
    }
    let fixture = try XCTUnwrap(chosen, "No open two-metre habitat route near Brookweaver")
    world.camera.position = SIMD3<Float>(
      fixture.player.x, world.groundHeight(fixture.player.x, fixture.player.y) + 1.72,
      fixture.player.y)
    world.camera.yaw = fixture.yaw
    world.camera.pitch = -0.08
    world.move(.zero)
    world.syncExpeditionPlayer()

    world.advance(1 / 60, running: false)
    XCTAssertTrue(world.controller.state.travel.observedAnimals.contains(actorID))
    _ = try world.control("water")
    world.advance(1 / 60, running: false)
    var fact = try XCTUnwrap(world.controller.state.population.rareHabitatFacts(
      observedAnimalIDs: world.controller.state.travel.observedAnimals,
      garden: world.controller.state.garden ?? HabitatGarden()).first)
    XCTAssertEqual(fact.state, .approachingPreferredHabitat)
    XCTAssertEqual(fact.structure?.state, .building)
    XCTAssertTrue(world.creatureCanTraverse(authoredActor.position, fixture.target))

    for _ in 0..<30 {
      let facts = world.controller.state.population.rareHabitatFacts(
        observedAnimalIDs: world.controller.state.travel.observedAnimals,
        garden: world.controller.state.garden ?? HabitatGarden())
      if facts.first?.state == .settledAtPreferredHabitat { break }
      world.advance(1, running: false)
    }
    fact = try XCTUnwrap(world.controller.state.population.rareHabitatFacts(
      observedAnimalIDs: world.controller.state.travel.observedAnimals,
      garden: world.controller.state.garden ?? HabitatGarden()).first)
    XCTAssertEqual(fact.state, .settledAtPreferredHabitat)

    for _ in 0..<200 {
      let facts = world.controller.state.population.rareHabitatFacts(
        observedAnimalIDs: world.controller.state.travel.observedAnimals,
        garden: world.controller.state.garden ?? HabitatGarden())
      if facts.first?.structure?.state == .active { break }
      world.advance(1, running: false)
    }
    fact = try XCTUnwrap(world.controller.state.population.rareHabitatFacts(
      observedAnimalIDs: world.controller.state.travel.observedAnimals,
      garden: world.controller.state.garden ?? HabitatGarden()).first)
    let active = try XCTUnwrap(fact.structure)
    XCTAssertEqual(active.kind, .dam)
    XCTAssertEqual(active.state, .active)

    _ = try world.control("grove")
    world.advance(1 / 60, running: false)
    fact = try XCTUnwrap(world.controller.state.population.rareHabitatFacts(
      observedAnimalIDs: world.controller.state.travel.observedAnimals,
      garden: world.controller.state.garden ?? HabitatGarden()).first)
    XCTAssertEqual(fact.structure?.id, active.id)
    XCTAssertEqual(fact.structure?.state, .displaced)

    _ = try world.control("undoPlanting")
    world.advance(1 / 60, running: false)
    fact = try XCTUnwrap(world.controller.state.population.rareHabitatFacts(
      observedAnimalIDs: world.controller.state.travel.observedAnimals,
      garden: world.controller.state.garden ?? HabitatGarden()).first)
    XCTAssertEqual(fact.structure?.id, active.id)
    XCTAssertEqual(fact.structure?.state, .rebuilding)

    for _ in 0..<200 {
      let facts = world.controller.state.population.rareHabitatFacts(
        observedAnimalIDs: world.controller.state.travel.observedAnimals,
        garden: world.controller.state.garden ?? HabitatGarden())
      if facts.first?.structure?.state == .active { break }
      world.advance(1, running: false)
    }
    fact = try XCTUnwrap(world.controller.state.population.rareHabitatFacts(
      observedAnimalIDs: world.controller.state.travel.observedAnimals,
      garden: world.controller.state.garden ?? HabitatGarden()).first)
    XCTAssertEqual(fact.structure?.id, active.id)
    XCTAssertEqual(fact.structure?.state, .active)
    XCTAssertGreaterThan(fact.structure?.revision ?? 0, active.revision)
  }
}
