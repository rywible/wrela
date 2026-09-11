import FieldCore
import XCTest
import simd

@testable import SanctuaryContent

final class ExpeditionTests: XCTestCase {
  func rescued() -> Expedition {
    var state = Expedition()
    state.player = Expedition.den
    for _ in 0..<300 { state.advance(seconds: 1 / 60, movingQuickly: false) }
    state.interact()
    return state
  }
  func testCompleteExpeditionAndHint() {
    var state = Expedition()
    for i in Expedition.signs.indices {
      state.player = Expedition.signs[i]
      state.interact()
      XCTAssertTrue(state.discoveredSigns.contains(i))
    }
    state.player = Expedition.den
    XCTAssertEqual(state.phase, .searching)
    state.interact()
    XCTAssertEqual(state.phase, .searching, "Rescue requires earned trust")
    for _ in 0..<300 { state.advance(seconds: 1 / 60, movingQuickly: false) }
    state.interact()
    XCTAssertEqual(state.phase, .carrying)
    state.interact()
    XCTAssertEqual(state.phase, .carrying, "Cannot release outside sanctuary")
    state.player = Expedition.home
    state.interact()
    XCTAssertEqual(state.phase, .settled)
    XCTAssertEqual(state.habitat, 0)
    for _ in 0..<1200 { state.advance(seconds: 1 / 60, movingQuickly: false) }
    XCTAssertEqual(state.habitat, 1)
    state.interact()
    XCTAssertTrue(state.legendaryHint)
    for _ in 0..<5 { state.interact() }
    XCTAssertEqual(
      state.phase, .settled, "Repeat interactions cannot duplicate or recapture the resident")
  }
  func testRunningAndOcclusionCannotEarnTrust() {
    var state = Expedition()
    state.player = Expedition.den
    for _ in 0..<300 { state.advance(seconds: 1 / 60, movingQuickly: true) }
    XCTAssertEqual(state.trust, 0)
    for _ in 0..<300 { state.advance(seconds: 1 / 60, movingQuickly: false, visible: false) }
    XCTAssertEqual(state.trust, 0)
    for _ in 0..<300 { state.advance(seconds: 1 / 60, movingQuickly: false) }
    state.interact(visible: false)
    XCTAssertEqual(state.phase, .searching)
    state.player = Expedition.home
    state.interact()
    XCTAssertEqual(state.phase, .searching)
  }
  func testExactSaveAndFutureReplay() throws {
    let original = rescued()
    var restored = try JSONDecoder().decode(Expedition.self, from: JSONEncoder().encode(original))
    XCTAssertEqual(restored, original)
    var reference = original
    for _ in 0..<30 {
      restored.advance(seconds: 1 / 60, movingQuickly: false)
      reference.advance(seconds: 1 / 60, movingQuickly: false)
    }
    XCTAssertEqual(restored, reference)
  }
  func testAtomicSaveRecoveryAndVersionRejection() throws {
    let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: folder) }
    let store = ExpeditionStore(url: folder.appendingPathComponent("save.json"))
    XCTAssertEqual(try store.load().state, Expedition())
    var state = rescued()
    try store.save(state)
    try store.save(state)
    try Data("broken".utf8).write(to: store.url)
    let recovered = try store.load()
    XCTAssertTrue(recovered.recovered)
    XCTAssertEqual(recovered.state, state)
    state.player = Expedition.home
    state.interact()
    try store.save(state)
    XCTAssertEqual(try store.load().state.phase, .settled)
    var data = try JSONSerialization.jsonObject(with: Data(contentsOf: store.url)) as! [String: Any]
    data["version"] = 99
    let future = try JSONSerialization.data(withJSONObject: data)
    try future.write(to: store.url)
    XCTAssertThrowsError(try store.load())
    XCTAssertThrowsError(try store.save(state))
    XCTAssertEqual(try Data(contentsOf: store.url), future)
  }
  func testInvalidStateAndForeignSlotPayloadRejected() throws {
    var state = Expedition()
    state.player.x = .nan
    XCTAssertThrowsError(try state.validate())
    let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: folder) }
    let store = ExpeditionStore(url: folder.appendingPathComponent("save.json"))
    try store.save(Expedition())
    var data = try JSONSerialization.jsonObject(with: Data(contentsOf: store.url)) as! [String: Any]
    data["creatureID"] = "different-creature"
    try JSONSerialization.data(withJSONObject: data).write(to: store.url)
    XCTAssertThrowsError(try store.load())
  }
}
