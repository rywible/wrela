import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class PersonalConstructionTests: XCTestCase {
  private func place(
    _ primitive: PersonalConstruction.Primitive, x: Float, y: Float = 0, z: Float,
    yaw: Float = 0, scale: Float = 1, in construction: inout PersonalConstruction
  ) throws -> PersonalConstruction.PlacementID {
    try construction.apply(
      .place(primitive, at: .init(x: x, y: y, z: z), yawRadians: yaw, scale: scale),
      expectedRevision: construction.revision)
  }

  func testRoundTripKeepsFutureIDsAndCollisionFacts() throws {
    var original = PersonalConstruction()
    _ = try place(.cabin, x: 0, z: 0, in: &original)
    let data = try JSONEncoder().encode(original)
    var restored = try JSONDecoder().decode(PersonalConstruction.self, from: data)
    XCTAssertEqual(restored, original)

    let originalID = try place(.deck, x: 10, z: 0, in: &original)
    let restoredID = try place(.deck, x: 10, z: 0, in: &restored)
    XCTAssertEqual(restoredID, originalID)
    XCTAssertEqual(restored.collisionFacts(at: .init(x: 2.75, y: 1, z: 0)).first?.kind, .solid)
    XCTAssertTrue(restored.collisionFacts(at: .init(x: 0, y: 1, z: -2.35)).isEmpty)
  }

  func testInvalidStaleAndOverlappingSolidCommandsAreAtomic() throws {
    var construction = PersonalConstruction()
    _ = try place(.cabin, x: 0, z: 0, in: &construction)
    let before = construction
    XCTAssertThrowsError(try construction.apply(
      .place(.bench, at: .init(x: 2.5, y: 0, z: 0), yawRadians: 0, scale: 1),
      expectedRevision: construction.revision))
    XCTAssertEqual(construction, before)
    XCTAssertThrowsError(try construction.apply(
      .place(.lantern, at: .init(x: Float.nan, y: 0, z: 4), yawRadians: 0, scale: 1),
      expectedRevision: construction.revision))
    XCTAssertEqual(construction, before)
    XCTAssertThrowsError(try construction.apply(.undo, expectedRevision: 0))
    XCTAssertEqual(construction, before)
  }

  func testWalkablePiecesCanMeetAndUndoIsRevisioned() throws {
    var construction = PersonalConstruction()
    let path = try place(.path, x: 0, z: 0, in: &construction)
    let bridge = try place(.bridge, x: 0, z: 0, in: &construction)
    XCTAssertEqual(construction.collisionFacts.filter { $0.kind == .walkable }.count, 2)
    XCTAssertEqual(try construction.apply(.undo, expectedRevision: construction.revision), bridge)
    XCTAssertEqual(construction.placements.map(\.id), [path])
    XCTAssertThrowsError(try construction.apply(.remove(999), expectedRevision: construction.revision))
  }

  func testWalkableProfilesMatchVisiblePlatformsAndRailsBlockOnlyTheirEdges() throws {
    let profiles: [(primitive: PersonalConstruction.Primitive,
      profile: SanctuaryWalkableConstructionProfile.Profile)] = [
      (.bridge, SanctuaryWalkableConstructionProfile.bridge),
      (.deck, SanctuaryWalkableConstructionProfile.deck),
    ]
    for (primitive, profile) in profiles {
      var construction = PersonalConstruction()
      // Minimum scale keeps the low horizontal bridge rail below the old 35 cm probe.
      let scale: Float = 0.5
      let base = PersonalConstruction.Location(x: 10, y: 3, z: -8)
      let id = try place(primitive, x: base.x, y: base.y, z: base.z,
        yaw: .pi / 4, scale: scale, in: &construction)
      let facts = construction.collisionFacts.filter { $0.placementID == id }
      let platform = try XCTUnwrap(facts.first { $0.kind == .walkable })
      XCTAssertEqual(platform.top, base.y + profile.platform.top * scale, accuracy: 0.0001)
      XCTAssertEqual(facts.filter { $0.kind == .solid }.count, profile.rails.count)

      // Rails remain solid, but the platform's centerline stays free for normal movement.
      let center = PersonalConstruction.Location(x: base.x, y: platform.top, z: base.z)
      XCTAssertFalse(construction.blocksStanding(at: center, eyeHeight: 1.72))
      let rail = profile.rails[0].bounds.center * scale
      let yaw: Float = .pi / 4
      let cosine = cos(yaw), sine = sin(yaw)
      let edge = PersonalConstruction.Location(
        x: base.x + rail.x * cosine + rail.z * sine,
        y: platform.top,
        z: base.z - rail.x * sine + rail.z * cosine)
      XCTAssertTrue(construction.blocksStanding(at: edge, eyeHeight: 1.72))

      let reopened = try JSONDecoder().decode(
        PersonalConstruction.self, from: JSONEncoder().encode(construction))
      XCTAssertEqual(reopened.placement(id: id), construction.placement(id: id))
      XCTAssertEqual(reopened.collisionFacts, facts)
    }
  }

  func testProductionPlayerCrossesBridgeCenterlineAndStopsAtRailChord() throws {
    let world = try SanctuaryWorld(seed: 17)
    let center = SIMD2<Float>(100, 100)
    // Player movement resolves a finite foot area, rather than one terrain point.
    // Give this direct production fixture the same five-probe support at its base
    // so the platform, instead of a 1 cm higher terrain probe, is authoritative.
    let supportBase = world.standingTerrainHeight(at: center, state: world.controller.state)
    _ = try world.commitPlacement(
      .place(.bridge,
        at: .init(x: center.x, y: supportBase, z: center.y),
        yawRadians: 0, scale: 0.5), expectedRevision: 0)
    let bridge = try XCTUnwrap(world.constructionFacts.first {
      $0.placementID == 1 && $0.kind == .walkable
    })
    func stand(_ point: SIMD2<Float>) {
      world.camera = .init(
        position: SIMD3(point.x, world.groundHeight(point.x, point.y) + 1.72, point.y),
        yaw: .pi / 2, pitch: 0)
      world.syncExpeditionPlayer()
    }

    // The production movement loop climbs the authored platform and reaches its clear center.
    stand(SIMD2(center.x - 2, center.y))
    world.move(SIMD3(0, 0, 2))
    XCTAssertGreaterThan(world.camera.position.x, center.x - 0.1)
    XCTAssertEqual(world.camera.position.y - 1.72, bridge.top, accuracy: 0.0001)

    // The same production movement cannot pass through the horizontal side rail.
    stand(SIMD2(center.x - 2, center.y + 0.36))
    world.move(SIMD3(0, 0, 2))
    XCTAssertLessThan(world.camera.position.x, center.x - 1.2)
  }

  func testLegacyEmptyDocumentDefaults() throws {
    let legacy = try JSONSerialization.data(withJSONObject: [:])
    let restored = try JSONDecoder().decode(PersonalConstruction.self, from: legacy)
    XCTAssertEqual(restored, PersonalConstruction())
  }

  func testUpdateKeepsPlacementIDAndUndoRestoresMoveResizeAndRemoval() throws {
    var construction = PersonalConstruction()
    let id = try place(.bench, x: 8, z: 0, in: &construction)
    _ = try construction.apply(
      .update(id, at: .init(x: 12, y: 0, z: 3), yawRadians: .pi / 4, scale: 1.5),
      expectedRevision: construction.revision)
    XCTAssertEqual(construction.placement(id: id)?.id, id)
    XCTAssertEqual(construction.placement(id: id)?.location, .init(x: 12, y: 0, z: 3))
    XCTAssertEqual(construction.placement(id: id)?.scale, 1.5)
    XCTAssertEqual(try construction.apply(.undo, expectedRevision: construction.revision), id)
    XCTAssertEqual(construction.placement(id: id)?.location, .init(x: 8, y: 0, z: 0))

    _ = try construction.apply(.remove(id), expectedRevision: construction.revision)
    XCTAssertNil(construction.placement(id: id))
    XCTAssertEqual(try construction.apply(.undo, expectedRevision: construction.revision), id)
    XCTAssertEqual(construction.placement(id: id)?.id, id)
  }

  func testUpdateRejectionAndStaleRevisionPreserveHistoryAndPlacements() throws {
    var construction = PersonalConstruction()
    let cabin = try place(.cabin, x: 0, z: 0, in: &construction)
    let bench = try place(.bench, x: 12, z: 0, in: &construction)
    let before = construction
    XCTAssertThrowsError(try construction.apply(
      .update(bench, at: .init(x: 2.5, y: 0, z: 0), yawRadians: 0, scale: 1),
      expectedRevision: construction.revision))
    XCTAssertEqual(construction, before)
    XCTAssertThrowsError(try construction.apply(
      .update(cabin, at: .init(x: 0, y: 0, z: 0), yawRadians: 0, scale: 1),
      expectedRevision: 0))
    XCTAssertEqual(construction, before)
  }

  func testLegacyPlacementDocumentUsesPreviousUndoBehaviorWithoutHistory() throws {
    var original = PersonalConstruction()
    let first = try place(.path, x: 0, z: 0, in: &original)
    let second = try place(.deck, x: 8, z: 0, in: &original)
    var legacy = try JSONSerialization.jsonObject(with: JSONEncoder().encode(original)) as! [String: Any]
    legacy.removeValue(forKey: "history")
    legacy.removeValue(forKey: "selectedPlacementID")
    var restored = try JSONDecoder().decode(
      PersonalConstruction.self, from: JSONSerialization.data(withJSONObject: legacy))
    XCTAssertTrue(restored.history.isEmpty)
    XCTAssertEqual(try restored.apply(.undo, expectedRevision: restored.revision), second)
    XCTAssertEqual(restored.placements.map(\.id), [first])
  }

  func testHistoryIsBoundedWhileRepeatedUpdatesKeepThePlacementStable() throws {
    var construction = PersonalConstruction()
    let id = try place(.lantern, x: 8, z: 0, in: &construction)
    for index in 0..<140 {
      _ = try construction.apply(
        .update(id, at: .init(x: index.isMultiple(of: 2) ? 8 : 9, y: 0, z: 0),
          yawRadians: 0, scale: 1), expectedRevision: construction.revision)
    }
    XCTAssertEqual(construction.history.count, PersonalConstruction.maximumHistoryCount)
    XCTAssertEqual(construction.placement(id: id)?.id, id)
  }

  func testDecodeRejectsHistoryWhoseAfterStateDoesNotMatchCurrentPlacement() throws {
    var construction = PersonalConstruction()
    let id = try place(.lantern, x: 8, z: 0, in: &construction)
    _ = try construction.apply(
      .update(id, at: .init(x: 9, y: 0, z: 0), yawRadians: 0, scale: 1),
      expectedRevision: construction.revision)
    var document = try JSONSerialization.jsonObject(with: JSONEncoder().encode(construction)) as! [String: Any]
    var history = document["history"] as! [[String: Any]]
    var last = history.removeLast()
    var after = last["after"] as! [String: Any]
    var location = after["location"] as! [String: Any]
    location["x"] = 99
    after["location"] = location
    last["after"] = after
    history.append(last)
    document["history"] = history
    XCTAssertThrowsError(try JSONDecoder().decode(
      PersonalConstruction.self, from: JSONSerialization.data(withJSONObject: document)))
  }
}
