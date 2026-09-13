import Foundation
import SimulationCore
import XCTest
import simd

@testable import SanctuaryContent

final class BoulderIntegrationTests: XCTestCase {
  func testWillingCompanionMovesSourceBoulderAndUndoRestoresCollisionAfterReload() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let world = try SanctuaryWorld(root: root)
    let helperID = "moonhart-001"
    // Boulder behavior requires an established companion, while pacing belongs
    // to the population relationship tests.
    try SanctuaryRelationshipFixture.establish(helperID, in: world)
    let helper = try XCTUnwrap(world.controller.state.population.actor(id: helperID))
    world.camera.position = SIMD3(helper.position.x,
      world.groundHeight(helper.position.x, helper.position.y + 4) + 1.72, helper.position.y + 4)
    world.camera.pitch = -0.08
    XCTAssertTrue(try XCTUnwrap(world.controller.state.population.actor(id: helperID)).companion.helpEligible)

    let geography = SanctuaryGeography()
    let feature = try XCTUnwrap(SanctuaryTerrainChunkKey.neighborhood(around: .zero)
      .flatMap { geography.plan(for: $0).features }.first {
        $0.kind == .boulder && $0.coordinate.x > 165
          && Terrain().water(at: $0.coordinate) == nil
      })
    let player = feature.coordinate + SIMD2<Float>(0, 6)
    let helperTarget = player + SIMD2<Float>(1, -2)
    // Fixture placement uses the production companion update; movement is not
    // the behavior under test. The action below uses native control semantics.
    try world.controller.editLiving { state in
      var population = state.population
      let start = try XCTUnwrap(population.actor(id: helperID)).position
      let steps = max(1, Int(ceil(distance(start, helperTarget) / 8)))
      for step in 1...steps {
        try population.updateCompanionPosition(id: helperID,
          position: start + (helperTarget - start) * Float(step) / Float(steps),
          using: .ride, expectedRevision: population.revision)
      }
      state.wildlife = population
    }
    world.camera.position = SIMD3(player.x, world.groundHeight(player.x, player.y) + 1.72, player.y)
    world.camera.yaw = 0
    world.syncExpeditionPlayer()
    _ = try world.request("please move this boulder")
    XCTAssertEqual(world.controller.state.movedBoulders.history.last?.helperID, helperID)
    let outcomeID = "\(feature.id):\(world.controller.state.movedBoulders.revision)"
    let memories = try XCTUnwrap(
      world.controller.state.population.actor(id: helperID)?.relationship.assistanceMemories)
    XCTAssertEqual(memories.last?.kind, .boulderMovement)
    XCTAssertEqual(memories.last?.outcomeID, outcomeID)
    let displaced = world.controller.state.movedBoulders.resolvedCoordinate(for: feature)
    XCTAssertEqual(displaced, feature.coordinate + SIMD2<Float>(0, -2))
    XCTAssertTrue(world.world.solids.contains {
      $0.name == "boulder" && distance(SIMD2($0.position.x, $0.position.z), displaced) < 0.01
    })
    let reopened = try SanctuaryWorld(root: root)
    XCTAssertEqual(reopened.controller.state.movedBoulders, world.controller.state.movedBoulders)
    XCTAssertEqual(
      reopened.controller.state.population.actor(id: helperID)?.relationship.assistanceMemories,
      memories)
    _ = try reopened.control("undoBoulder")
    XCTAssertEqual(reopened.controller.state.movedBoulders.resolvedCoordinate(for: feature), feature.coordinate)
    XCTAssertTrue(reopened.world.solids.contains {
      $0.name == "boulder" && distance(SIMD2($0.position.x, $0.position.z), feature.coordinate) < 0.01
    })
  }

  func testHelpWithoutCapableCompanionIsAtomic() throws {
    let world = try SanctuaryWorld()
    let before = try world.checkpoint()
    XCTAssertThrowsError(try world.control("help-boulder"))
    XCTAssertEqual(try world.checkpoint(), before)
  }

  func testBlockedPushPreservesBoulderAndAssistanceMemory() throws {
    let setup = try preparedBoulderWorld()
    let destination = setup.feature.coordinate + SIMD2<Float>(0, -2)
    try setup.world.controller.editLiving { state in
      var construction = state.buildings
      _ = try construction.apply(
        .place(.bench,
          at: .init(
            x: destination.x,
            y: setup.world.groundHeight(destination.x, destination.y),
            z: destination.y),
          yawRadians: 0, scale: 1),
        expectedRevision: construction.revision)
      state.construction = construction
    }
    let before = try setup.world.checkpoint()
    let memories = setup.world.controller.state.population.actor(id: setup.helperID)?
      .relationship.assistanceMemories

    XCTAssertThrowsError(try setup.world.request("please move this boulder"))
    XCTAssertEqual(try setup.world.checkpoint(), before)
    XCTAssertEqual(
      setup.world.controller.state.population.actor(id: setup.helperID)?.relationship.assistanceMemories,
      memories)
  }

  func testFailedBoulderSaveDoesNotPublishPushOrAssistance() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let setup = try preparedBoulderWorld(root: root)
    let before = try setup.world.checkpoint()
    let memories = setup.world.controller.state.population.actor(id: setup.helperID)?
      .relationship.assistanceMemories
    let saves = root.appendingPathComponent("saves")
    if FileManager.default.fileExists(atPath: saves.path) {
      try FileManager.default.removeItem(at: saves)
    }
    // A file in place of the save directory forces the production store write
    // to fail after the action has built its complete candidate.
    try Data("unwritable save directory".utf8).write(to: saves)

    XCTAssertThrowsError(try setup.world.request("please move this boulder"))
    XCTAssertEqual(try setup.world.checkpoint(), before)
    XCTAssertEqual(
      setup.world.controller.state.population.actor(id: setup.helperID)?.relationship.assistanceMemories,
      memories)
  }

  private func preparedBoulderWorld(
    root: URL? = nil
  ) throws -> (world: SanctuaryWorld, helperID: String, feature: SanctuaryChunkFeature) {
    let world = try SanctuaryWorld(root: root)
    let helperID = "moonhart-001"
    try SanctuaryRelationshipFixture.establish(helperID, in: world)
    let geography = SanctuaryGeography()
    let feature = try XCTUnwrap(SanctuaryTerrainChunkKey.neighborhood(around: .zero)
      .flatMap { geography.plan(for: $0).features }.first {
        $0.kind == .boulder && $0.coordinate.x > 165
          && Terrain().water(at: $0.coordinate) == nil
      })
    let player = feature.coordinate + SIMD2<Float>(0, 6)
    let helperTarget = player + SIMD2<Float>(1, -2)
    try world.controller.editLiving { state in
      var population = state.population
      let start = try XCTUnwrap(population.actor(id: helperID)).position
      let steps = max(1, Int(ceil(distance(start, helperTarget) / 8)))
      for step in 1...steps {
        try population.updateCompanionPosition(id: helperID,
          position: start + (helperTarget - start) * Float(step) / Float(steps),
          using: .ride, expectedRevision: population.revision)
      }
      state.wildlife = population
    }
    world.camera.position = SIMD3(player.x, world.groundHeight(player.x, player.y) + 1.72, player.y)
    world.camera.yaw = 0
    world.camera.pitch = -0.08
    world.syncExpeditionPlayer()
    return (world, helperID, feature)
  }
}
