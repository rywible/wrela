import Foundation
import XCTest
@testable import SanctuaryContent

final class NaturePersistenceTests: XCTestCase {
  func testLegacyDiskSaveMigratesOnNextSuccessfulWrite() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let store = ExpeditionStore(url: root.appendingPathComponent("legacy.json"))
    var oldState = try JSONSerialization.jsonObject(with: JSONEncoder().encode(Expedition())) as! [String: Any]
    oldState.removeValue(forKey: "relationship")
    oldState.removeValue(forKey: "garden")
    let oldBytes = try JSONSerialization.data(withJSONObject: [
      "version": 1, "creatureID": Expedition.creatureID, "expedition": oldState,
    ])
    try oldBytes.write(to: store.url)
    var state = try store.load().state
    _ = try state.applyNature(.plant(.flowers, at: .init(x: 0, z: 20), radius: 2))
    try store.save(state)
    let document = try JSONSerialization.jsonObject(with: Data(contentsOf: store.url)) as! [String: Any]
    XCTAssertEqual(document["version"] as? Int, 3)
    XCTAssertEqual(try Data(contentsOf: store.backupURL), oldBytes)
    XCTAssertEqual(try store.load().state, state)
  }
  func testPlantingSurvivesWorldCheckpointAndDiskReopen() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let world = try SanctuaryWorld(root: root)
    let command: HabitatGarden.Command = .plant(.flowers, at: .init(x: 0, z: 20), radius: 2)
    _ = try world.controller.applyNature(command, expectedRevision: 0)
    let checkpoint = try world.checkpoint()
    let saved = world.controller.state.garden
    _ = try world.controller.applyNature(.undo, expectedRevision: 1)
    XCTAssertEqual(world.controller.state.garden?.patches.count, 0)
    try world.restore(checkpoint)
    XCTAssertEqual(world.controller.state.garden, saved)
    try world.controller.save()
    let reopened = try SanctuaryWorld(root: root)
    XCTAssertEqual(reopened.controller.state.garden, saved)
    XCTAssertEqual(try reopened.checkpoint(), checkpoint)
  }

  func testFailedDiskWritePreservesGardenAndRelationship() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let controller = try ExpeditionController(root: root)
    controller.updatePlayer(SIMD3(-3, 1.72, -83), yaw: 0, pitch: 0)
    let before = try controller.checkpoint()
    try Data("not a directory".utf8).write(to: root.appendingPathComponent("saves"))
    XCTAssertThrowsError(try controller.applyNature(
      .plant(.flowers, at: .init(x: 0, z: 20), radius: 2), expectedRevision: 0))
    XCTAssertEqual(try controller.checkpoint(), before)
    XCTAssertThrowsError(try controller.addressCreature("hello", visible: true))
    XCTAssertEqual(try controller.checkpoint(), before)
  }

  func testStaleEditPreservesControllerCheckpoint() throws {
    let controller = try ExpeditionController()
    _ = try controller.applyNature(.plant(.flowers, at: .init(x: 0, z: 20), radius: 2), expectedRevision: 0)
    let before = try controller.checkpoint()
    XCTAssertThrowsError(try controller.applyNature(.undo, expectedRevision: 0))
    XCTAssertEqual(try controller.checkpoint(), before)
  }
}
