import FieldCore
import Foundation
import XCTest

@testable import SanctuaryContent

final class PersistenceFailureTests: XCTestCase {
  func testFailedRescueWriteDoesNotCommitCandidate() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let controller = try ExpeditionController(root: root)
    controller.updatePlayer(V3(-3, 1.72, -80), yaw: 0, pitch: 0)
    for _ in 0..<300 { controller.advance(1 / 60, running: false, visible: true, perceived: true) }
    let before = try controller.checkpoint()
    // A file at the directory location deterministically injects a real filesystem error.
    let saves = root.appendingPathComponent("saves")
    if FileManager.default.fileExists(atPath: saves.path) {
      try FileManager.default.removeItem(at: saves)
    }
    try Data("unwritable save directory".utf8).write(to: saves)
    XCTAssertThrowsError(try controller.interact(visible: true))
    XCTAssertEqual(try controller.checkpoint(), before)
  }
}

extension PersistenceFailureTests {
  func testDiskRoundTripKeepsCollisionResolvedElevation() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let world = try SanctuaryWorld(root: root)
    for _ in 0..<40 { world.move(V3(0, 0, 0.1)) }
    world.syncExpeditionPlayer()
    try world.controller.save()
    let before = world.camera
    try world.loadExpeditionSlot("other")
    try world.loadExpeditionSlot("expedition")
    XCTAssertEqual(world.camera, before)
    let restarted = try SanctuaryWorld(root: root)
    XCTAssertEqual(restarted.camera, before)
  }
  func testLegacySaveWithoutElevationStillDecodes() throws {
    let state = Expedition()
    let bytes = try JSONEncoder().encode(state)
    XCTAssertNil(try JSONDecoder().decode(Expedition.self, from: bytes).playerElevation)
    var invalid = state
    invalid.playerElevation = .infinity
    XCTAssertThrowsError(try invalid.validate())
  }
}
