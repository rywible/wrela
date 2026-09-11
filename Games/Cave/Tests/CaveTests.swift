import FieldCore
import XCTest
import simd

@testable import CaveContent

final class CaveTests: XCTestCase {
  func testVisibilityNeedsAnUnoccludedBeam() {
    var a = CaveSimulation()
    for _ in 0..<180 { a.step(player: V3(0, 1.72, -13), forward: V3(0, 0, -1), illuminated: false) }
    XCTAssertEqual(a.encounter, "watching")
    XCTAssertEqual(a.scares, 0)
    for _ in 0..<20 { a.step(player: V3(0, 1.72, -13), forward: V3(0, 0, -1), illuminated: true) }
    XCTAssertEqual(a.scares, 1)
    XCTAssertEqual(a.encounter, "withdrawing")
    for _ in 0..<180 { a.step(player: V3(0, 1.72, -13), forward: V3(0, 0, -1), illuminated: true) }
    XCTAssertEqual(a.scares, 1)
  }
  func testObjectiveReturnScareAndEscape() {
    var a = CaveSimulation()
    XCTAssertFalse(a.recoverBeacon(player: CaveLayout.entrance))
    XCTAssertTrue(a.recoverBeacon(player: CaveLayout.relic + V3(0, 1, 0)))
    a.step(player: V3(3, 1.72, -26), forward: V3(0, 0, -1), illuminated: false)
    XCTAssertFalse(a.returnedScare)
    a.step(player: V3(3, 1.72, -26), forward: V3(0, 0, -1), illuminated: true)
    XCTAssertEqual(a.encounter, "rush")
    a.step(player: CaveLayout.entrance, forward: V3(0, 0, 1), illuminated: true)
    XCTAssertEqual(a.phase, "escaped")
  }
  func testSaveReplaysIdenticalFuture() throws {
    var a = CaveSimulation(seed: 71)
    for _ in 0..<15 { a.step(player: V3(0, 1.72, -13), forward: V3(0, 0, -1), illuminated: true) }
    var b = try JSONDecoder().decode(CaveSimulation.self, from: JSONEncoder().encode(a))
    for _ in 0..<100 {
      a.step(player: V3(0, 1.72, -13), forward: V3(0, 0, -1), illuminated: true)
      b.step(player: V3(0, 1.72, -13), forward: V3(0, 0, -1), illuminated: true)
    }
    XCTAssertEqual(a, b)
    try b.validate()
  }
  func testVolumetricRockAndVisibility() {
    let rock = CaveLayout.rock()
    XCTAssertGreaterThan(rock.value(at: CaveLayout.entrance), 0)
    XCTAssertLessThan(rock.value(at: V3(0, 6, 0)), 0)
    XCTAssertLessThan(rock.value(at: V3(0, -0.5, 0)), 0)
    XCTAssertTrue(FieldQueries.visible(from: V3(0, 1.7, 2), to: V3(0, 1.7, -8), solid: rock))
    XCTAssertFalse(FieldQueries.visible(from: V3(0, 1.7, 0), to: V3(8, 1.7, -22), solid: rock))
  }
  func testCapsuleCannotWalkThroughWallsOrCeilings() {
    let rock = CaveLayout.rock()
    let start = V3(0, 1.72, 0)
    let moved = FieldQueries.moveCapsule(eye: start, delta: V3(10, 0, 0), solid: rock)
    XCTAssertLessThan(moved.x, 3)
    XCTAssertGreaterThan(rock.value(at: moved), 0.2)
    let ceiling = FieldQueries.moveCapsule(eye: start, delta: V3(0, 10, 0), solid: rock)
    XCTAssertLessThan(ceiling.y, 4.5)
    XCTAssertGreaterThan(rock.value(at: ceiling), 0.2)
  }
}
