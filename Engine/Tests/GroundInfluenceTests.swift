import XCTest
import FieldCore
import simd

final class GroundInfluenceTests: XCTestCase {
  private func contact(id: UInt64 = 1, height: Float = 0, time: Double = 10) -> GroundInfluenceEvent {
    GroundInfluenceEvent(id: id, sourceID: "player", kind: .foot,
      start: SIMD3(-0.25, height, 0), end: SIMD3(0.25, height, 0), radius: 0.6,
      displacement: 0.3, compression: 0.7, startTime: time - 0.1, endTime: time,
      recoverySeconds: 1.5)
  }

  func testContactHasCompactSupportAndRejectsOtherElevations() throws {
    var field = GroundInfluenceField()
    try field.update(.init(time: 10, events: [contact(height: 3)]), center: .zero)
    XCTAssertGreaterThan(length(field.sample(at: SIMD3(0.2, 3, 0.15))), 0.1)
    XCTAssertEqual(field.sample(at: SIMD3(0.2, 0, 0.15)), .zero)
    XCTAssertEqual(field.sample(at: SIMD3(0.2, 4, 0.15)), .zero)
    XCTAssertEqual(field.sample(at: SIMD3(2, 3, 2)), .zero)
    XCTAssertGreaterThan(field.activeCellCount, 0)
    XCTAssertLessThan(field.visitedCells, 100)
  }

  func testRecenterPreservesWorldContactsWithoutPeriodicCopies() throws {
    let snapshot = SurfaceInfluenceSnapshot(time: 10, events: [contact()])
    var field = GroundInfluenceField()
    try field.update(snapshot, center: .zero)
    let root = SIMD3<Float>(0.2, 0, 0.15), value = field.sample(at: root)
    try field.update(snapshot, center: SIMD2(1.25, -0.5))
    XCTAssertLessThan(length(field.sample(at: root) - value), 0.00001)
    try field.update(snapshot, center: SIMD2(64, 0))
    XCTAssertEqual(field.activeCellCount, 0)
    XCTAssertEqual(field.sample(at: SIMD3(64.2, 0, 0.15)), .zero)
    try field.update(snapshot, center: .zero)
    XCTAssertEqual(field.sample(at: root), value)
  }

  func testSubCellFootContactCannotFallBetweenGridSamples() throws {
    var event = contact(); event.start = .zero; event.end = .zero; event.radius = 0.08
    var field = GroundInfluenceField()
    try field.update(.init(time: 10, events: [event]), center: .zero)
    XCTAssertGreaterThan(field.sample(at: .zero).z, 0)
  }

  func testRecoveryAndLargeSavedTimesRetainSubsecondPrecision() throws {
    let base = 999_999_999.0
    let event = contact(time: base)
    XCTAssertLessThan(event.response(at: base + 0.25), event.response(at: base))
    XCTAssertLessThan(event.response(at: base + 0.5), event.response(at: base + 0.25))
    XCTAssertEqual(event.response(at: base - 1), 0)
    let original = contact()
    XCTAssertEqual(original.response(at: 22), 0)
    var field = GroundInfluenceField()
    try field.update(.init(time: 22, events: [original]), center: .zero)
    XCTAssertEqual(field.activeCellCount, 0)
  }

  func testSerializationAndEventOrderReplaySameGrid() throws {
    var second = contact(id: 2); second.end.z = 0.3
    let snapshot = SurfaceInfluenceSnapshot(time: 10.25, events: [second, contact()])
    let restored = try JSONDecoder().decode(SurfaceInfluenceSnapshot.self,
      from: JSONEncoder().encode(snapshot))
    var a = GroundInfluenceField(), b = GroundInfluenceField()
    try a.update(snapshot, center: .zero)
    try b.update(.init(time: restored.time, events: Array(restored.events.reversed())), center: .zero)
    XCTAssertEqual(a.origin, b.origin)
    XCTAssertEqual(a.cells, b.cells)
    XCTAssertEqual(MemoryLayout<GroundInfluenceCell>.stride, 32)
  }

  func testRejectsOversizedAndInvalidHistoryWithoutMutatingField() throws {
    var field = GroundInfluenceField()
    try field.update(.init(time: 10, events: [contact()]), center: .zero)
    let cells = field.cells, origin = field.origin
    let tooMany = (0...256).map { contact(id: UInt64($0)) }
    XCTAssertThrowsError(try field.update(.init(time: 10, events: tooMany), center: SIMD2(5, 5)))
    var bad = contact(); bad.end.y = .nan
    XCTAssertThrowsError(try field.update(.init(time: 10, events: [bad]), center: .zero))
    XCTAssertThrowsError(try field.update(.init(time: 10, events: [contact(), contact()]), center: .zero))
    XCTAssertEqual(field.cells, cells)
    XCTAssertEqual(field.origin, origin)
  }

  func testDifferentSupportLayersNeverBlendIntoIntermediateHeight() throws {
    var floor = contact(), roof = contact(id: 2, height: 3)
    floor.compression = 0.2; roof.compression = 1
    var field = GroundInfluenceField()
    try field.update(.init(time: 10, events: [floor, roof]), center: .zero)
    XCTAssertEqual(field.sample(at: SIMD3(0.2, 1.5, 0.15)), .zero)
    XCTAssertEqual(field.sample(at: SIMD3(0.2, 0, 0.15)), .zero)
    XCTAssertGreaterThan(length(field.sample(at: SIMD3(0.2, 3, 0.15))), 0.1)
  }

  func testRotationPreservesRootAndRespectsCullingBound() {
    for radius: Float in [0.1, 0.6, 1.8, 4, 12] {
      let rotation = GroundInfluenceField.rotation(SIMD3(1.5, 0.5, 1), bladeRadius: radius)
      let axis = SIMD3(rotation.x, rotation.y, rotation.z), angle = rotation.w
      func rotate(_ v: SIMD3<Float>) -> SIMD3<Float> {
        v * cos(angle) + cross(axis, v) * sin(angle) + axis * dot(axis, v) * (1 - cos(angle))
      }
      XCTAssertEqual(rotate(.zero), .zero)
      let tip = SIMD3<Float>(0, radius, 0)
      XCTAssertEqual(length(rotate(tip)), radius, accuracy: 0.00001)
      XCTAssertLessThanOrEqual(length(rotate(tip) - tip),
        GroundInfluenceField.maximumVertexDisplacement + 0.00001)
    }
  }
}
