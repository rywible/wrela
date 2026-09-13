import Foundation
import XCTest
import simd
@testable import SanctuaryContent

final class TerrainSaveReconciliationTests: XCTestCase {
  func testDiskReopenRegroundsObsoleteElevationWithoutLosingSavedEdits() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let world = try SanctuaryWorld(root: root)
    let point = SIMD2<Float>(-427, 10_745.171875)
    world.camera.position = SIMD3(point.x, world.groundHeight(point.x, point.y) + 1.72, point.y)
    world.syncExpeditionPlayer()
    _ = try world.controller.applyNature(.plant(.flowers, at: .init(x: -420, z: 10_740), radius: 2), expectedRevision: 0)
    // Simulate an older content revision's valid persisted ground elevation.
    try world.controller.editLiving { state in state.playerElevation = 182.44537 }
    let saved = world.controller.state
    let reopened = try SanctuaryWorld(root: root)
    XCTAssertEqual(reopened.controller.state.garden, saved.garden)
    XCTAssertEqual(reopened.controller.state.population, saved.population)
    XCTAssertEqual(reopened.controller.state.travel, saved.travel)
    XCTAssertEqual(reopened.controller.state.player, point)
    XCTAssertEqual(reopened.camera.position.y,
      reopened.standingTerrainHeight(at: point, state: saved) + 1.72, accuracy: 0.001)
    let checkpoint = try reopened.checkpoint()
    let replay = try SanctuaryWorld()
    try replay.restore(checkpoint)
    XCTAssertEqual(try replay.checkpoint(), checkpoint)
  }

  func testDiskReopenPreservesAnAlreadySupportedPoseExactly() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let world = try SanctuaryWorld(root: root)
    world.move(.zero)
    world.syncExpeditionPlayer()
    try world.controller.save()
    let reopened = try SanctuaryWorld(root: root)
    XCTAssertEqual(reopened.camera, world.camera)
    XCTAssertEqual(reopened.controller.state, world.controller.state)
  }
}
