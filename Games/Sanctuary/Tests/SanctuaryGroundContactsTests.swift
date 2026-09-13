import FieldCore
import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class SanctuaryGroundContactsTests: XCTestCase {
  private func place(_ world: SanctuaryWorld, at point: SIMD2<Float>) {
    world.camera.position = SIMD3(point.x, world.groundHeight(point.x, point.y) + 1.72, point.y)
    world.camera.yaw = 0
    world.syncExpeditionPlayer()
  }

  private func companionWorld(_ id: String, control: String) throws -> SanctuaryWorld {
    let world = try SanctuaryWorld(seed: 17)
    try SanctuaryRelationshipFixture.establish(id, in: world)
    let actor = try XCTUnwrap(world.controller.state.population.actor(id: id))
    place(world, at: actor.position + SIMD2<Float>(0, 4))
    _ = try world.control(control)
    return world
  }

  func testAcceptedWalkingUsesResolvedSupportAndCameraPlacementDoesNotCreateTrail() throws {
    let world = try SanctuaryWorld(seed: 17)
    place(world, at: SIMD2(40, 40))
    XCTAssertNil(world.controller.state.groundContacts)

    world.move(.zero)
    XCTAssertNil(world.controller.state.groundContacts, "A fixture or camera placement is not movement")

    let start = world.camera.position
    world.move(SIMD3<Float>(0, 0, 0.8))
    let contacts = try XCTUnwrap(world.controller.state.groundContacts)
    let body = try XCTUnwrap(contacts.events.first { $0.kind == .body })
    let foot = try XCTUnwrap(contacts.events.first { $0.kind == .foot })
    XCTAssertEqual(body.sourceID, SanctuaryGroundContacts.playerSourceID)
    XCTAssertEqual(body.start.x, start.x, accuracy: 0.001)
    XCTAssertEqual(body.start.z, start.z, accuracy: 0.001)
    XCTAssertEqual(body.start.y, start.y - 1.72, accuracy: 0.001)
    XCTAssertEqual(body.end.y, world.camera.position.y - 1.72, accuracy: 0.001)
    XCTAssertLessThanOrEqual(distance(body.start, body.end), 2)
    XCTAssertEqual(foot.start, foot.end, "A planted footprint is immutable point contact")
    XCTAssertNoThrow(try world.surfaceInfluenceSnapshot.validate())

    let planted = foot
    world.move(SIMD3<Float>(0, 0, 0.2))
    XCTAssertEqual(
      try XCTUnwrap(world.controller.state.groundContacts?.events.first { $0.id == planted.id }),
      planted)
  }

  func testBoundsRejectedMovementCreatesNoContact() throws {
    let world = try SanctuaryWorld(seed: 17)
    world.legacyInteractions = true
    let edge = SIMD2<Float>(SanctuaryGeography.bounds.maximum.x, 0)
    place(world, at: edge)
    world.move(SIMD3<Float>(1, 0, 0))
    XCTAssertEqual(SIMD2(world.camera.position.x, world.camera.position.z), edge)
    XCTAssertNil(world.controller.state.groundContacts)
  }

  func testGroundedRideUsesMountSourceAndFlightIsSuppressed() throws {
    let rider = try companionWorld("moonhart-001", control: "ride")
    rider.move(SIMD3<Float>(0, 0, 0.5))
    let ride = try XCTUnwrap(rider.controller.state.groundContacts?.events.last)
    XCTAssertEqual(ride.sourceID, "moonhart-001")
    XCTAssertEqual(ride.kind, .body)

    let flier = try companionWorld("canopy-glider-001", control: "fly")
    flier.move(SIMD3<Float>(0, 0, 0.5))
    XCTAssertNil(flier.controller.state.groundContacts)
  }

  func testAcceptedWildlifeSimulationMovementCreatesNearbyBodyContact() throws {
    let world = try SanctuaryWorld(seed: 17)
    let actor = try XCTUnwrap(world.controller.state.population.actor(id: Expedition.creatureID))
    place(world, at: actor.position + SIMD2<Float>(0, 6))
    world.camera.yaw = .pi // Keep the fixture outside the animal's visible calm-presence branch.
    for _ in 0..<200 { world.advance(1 / 60, running: false) }
    XCTAssertTrue(world.controller.state.recentGroundContacts.events.contains {
      $0.sourceID == actor.id && $0.kind == .body
    })
    XCTAssertFalse(world.controller.state.recentGroundContacts.events.contains {
      $0.sourceID == SanctuaryGroundContacts.playerSourceID
    })
  }

  func testCheckpointRestorePreservesSimulationAgedSnapshotExactly() throws {
    let world = try SanctuaryWorld(seed: 17)
    world.legacyInteractions = true
    world.move(SIMD3<Float>(0, 0, 0.8))
    world.advance(1, running: false)
    let expected = world.controller.state.recentGroundContacts.snapshot(at: world.controller.state.age)
    let saved = try world.checkpoint()

    world.move(SIMD3<Float>(0.4, 0, 0))
    try world.restore(saved)
    XCTAssertEqual(
      world.controller.state.recentGroundContacts.snapshot(at: world.controller.state.age), expected)
  }

  func testStrideSideAndTurnCadenceReplayFromCheckpoint() throws {
    let world = try SanctuaryWorld(seed: 17)
    world.legacyInteractions = true
    place(world, at: SIMD2(40, 40))
    world.move(SIMD3<Float>(0, 0, 0.4))
    let first = try XCTUnwrap(world.controller.state.groundContacts?.events.first {
      $0.kind == .foot
    })
    XCTAssertFalse(world.controller.state.recentGroundContacts.nextPlayerFootIsLeft)
    let checkpoint = try world.checkpoint()

    world.camera.yaw = .pi / 2
    world.move(SIMD3<Float>(0, 0, 0.6))
    let expected = world.controller.state.recentGroundContacts
    let turned = try XCTUnwrap(expected.events.last(where: {
      $0.kind == .foot && $0.id != first.id
    }))
    XCTAssertNotEqual(turned.heading, first.heading)
    XCTAssertTrue(expected.nextPlayerFootIsLeft)

    try world.restore(checkpoint)
    world.camera.yaw = .pi / 2
    world.move(SIMD3<Float>(0, 0, 0.6))
    XCTAssertEqual(world.controller.state.recentGroundContacts, expected)
  }

  func testHistoryCapsOldestEventsAndRejectsInvalidSaveAtomically() throws {
    var history = SanctuaryGroundContacts()
    for index in 0..<300 {
      let x = Float(index % 100)
      history.record(
        sourceID: SanctuaryGroundContacts.playerSourceID, kind: .body,
        start: SIMD3(x, 0, 0), end: SIMD3(x + 0.1, 0, 0),
        radius: 0.13, displacement: 0.02, compression: 0.7,
        at: Double(index), recoverySeconds: 45)
    }
    XCTAssertEqual(history.events.count, SurfaceInfluenceSnapshot.maximumEvents)
    XCTAssertEqual(history.events.last?.id, 300)
    XCTAssertGreaterThan(history.events.first?.id ?? 0, 1)

    let controller = try ExpeditionController(seed: 17)
    let before = try controller.checkpoint()
    let outside = SanctuaryGeography.bounds.maximum.x + 1
    let invalid = GroundInfluenceEvent(
      id: 1, sourceID: SanctuaryGroundContacts.playerSourceID, kind: .foot,
      start: SIMD3(outside, 0, 0), end: SIMD3(outside + 0.1, 0, 0),
      radius: 0.13, displacement: 0.02, compression: 0.7,
      startTime: 0, endTime: 0, recoverySeconds: 45)
    XCTAssertThrowsError(try controller.editLiving { state in
      state.groundContacts = SanctuaryGroundContacts(events: [invalid], nextID: 2)
    })
    XCTAssertEqual(try controller.checkpoint(), before)
  }

  func testLegacyExpeditionWithoutGroundContactFieldRemainsReadable() throws {
    let encoded = try JSONEncoder().encode(Expedition(seed: 17))
    var document = try XCTUnwrap(
      JSONSerialization.jsonObject(with: encoded) as? [String: Any])
    document.removeValue(forKey: "groundContacts")
    let restored = try JSONDecoder().decode(
      Expedition.self, from: JSONSerialization.data(withJSONObject: document))
    XCTAssertNil(restored.groundContacts)
    XCTAssertTrue(restored.recentGroundContacts.events.isEmpty)
  }
}
