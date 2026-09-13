import FieldCore
import Foundation
import XCTest
import simd
@testable import SanctuaryContent

final class SanctuaryTargetingTests: XCTestCase {
  private func meadowWorld() throws -> SanctuaryWorld {
    let world = try SanctuaryWorld()
    world.camera.position = V3(2_400, world.groundHeight(2_400, -740) + 1.72, -740)
    world.camera.yaw = 0
    world.camera.pitch = -0.8
    return world
  }

  func testCameraPitchFindsActualGroundAndUpwardViewDoesNotInventSupport() throws {
    let world = try meadowWorld()
    let target = world.cameraTarget(maxReach: 6)
    XCTAssertEqual(target.hitKind, .ground)
    XCTAssertEqual(target.obstruction, .none)
    let support = try XCTUnwrap(target.support)
    XCTAssertEqual(support.point.y, world.groundHeight(support.point.x, support.point.z), accuracy: 0.001)
    XCTAssertLessThan(target.distance, 6)
    XCTAssertGreaterThan(target.distance, 0)
    XCTAssertLessThan(distance(target.point, world.camera.position + world.camera.forward * target.distance), 0.001)
    world.camera.pitch = 0.8
    let sky = world.cameraTarget(maxReach: 6)
    XCTAssertEqual(sky.obstruction, .rangeLimit)
    XCTAssertNil(sky.support)
    XCTAssertEqual(sky.distance, 6)
    XCTAssertGreaterThan(sky.point.y, world.camera.position.y)
  }

  func testThinRotatedConstructionBlocksAndSelectedObjectCanBeIgnored() throws {
    let world = try meadowWorld()
    let origin = world.camera.position
    var construction = PersonalConstruction()
    let id = try construction.apply(.place(.fence,
      at: .init(x: origin.x, y: origin.y - 0.8, z: origin.z - 2.321),
      yawRadians: 0.3, scale: 0.5), expectedRevision: 0)
    try world.controller.editLiving { $0.construction = construction }
    let direction = V3(0, -0.15, -1)
    let blocked = world.target(origin: origin, direction: direction, maxReach: 6)
    XCTAssertEqual(blocked.hitKind, .construction)
    XCTAssertEqual(blocked.obstruction, .solid)
    XCTAssertEqual(blocked.placementID, id)
    XCTAssertNil(blocked.support)
    XCTAssertLessThan(blocked.distance, 2.5)
    XCTAssertGreaterThan(blocked.distance, 2)
    let ignored = world.target(origin: origin, direction: direction, maxReach: 6, ignoringPlacementID: id)
    XCTAssertGreaterThan(ignored.distance, blocked.distance + 0.1)
    XCTAssertNil(ignored.placementID)
  }

  func testOnlyWalkableConstructionTopSuppliesSupport() throws {
    let world = try meadowWorld()
    let p = world.camera.position
    let base = world.groundHeight(p.x, p.z)
    var construction = PersonalConstruction()
    let id = try construction.apply(.place(.deck, at: .init(x: p.x, y: base + 0.2, z: p.z),
      yawRadians: 0.7, scale: 1), expectedRevision: 0)
    try world.controller.editLiving { $0.construction = construction }
    let target = world.target(origin: V3(p.x, base + 3, p.z), direction: V3(0, -1, 0), maxReach: 6)
    XCTAssertEqual(target.hitKind, .walkableConstruction)
    XCTAssertEqual(target.obstruction, .none)
    XCTAssertEqual(target.placementID, id)
    XCTAssertEqual(try XCTUnwrap(target.support).point.y, base + 0.48, accuracy: 0.001)
    let side = world.target(origin: V3(p.x, base + 0.3, p.z + 4), direction: V3(0, 0, -1), maxReach: 6)
    XCTAssertEqual(side.obstruction, .solid)
    XCTAssertNil(side.support)
  }

  func testProductionShapeIntervalCannotSkipFourMillimetreSolid() throws {
    let world = try meadowWorld()
    let origin = world.camera.position
    world.world.solids.append(Solid(shape: .box(V3(0.3, 0.3, 0.002)),
      position: origin + V3(0, 0, -2.137), scale: 1, name: "Thin source fixture"))
    // Use the real production Solid/Shape source. The complete bounds pass must also find
    // a new solid before its point-grid cache has been rebuilt.
    let hit = world.target(origin: origin, direction: V3(0, 0, -1), maxReach: 3)
    XCTAssertEqual(hit.obstruction, .solid)
    XCTAssertEqual(hit.solidName, "Thin source fixture")
    XCTAssertEqual(hit.distance, 2.135, accuracy: 0.01)
    XCTAssertNil(hit.support)
  }

  func testCurrentBankEditChangesTargetAndWaterDoesNotHideBed() throws {
    let world = try SanctuaryWorld()
    let x: Float = 1_650, z: Float = 2_170
    world.camera.position = V3(x, world.groundHeight(x, z) + 4, z)
    let before = world.target(origin: world.camera.position, direction: V3(0, -1, 0), maxReach: 6)
    XCTAssertEqual(before.obstruction, .none)
    _ = try world.controller.applyNature(.sculpt(.raise, at: .init(x: x, z: z),
      radius: 3, amount: 1, targetHeight: nil), expectedRevision: 0)
    _ = try world.controller.applyNature(.plant(.shallowWater, at: .init(x: x, z: z), radius: 2),
      expectedRevision: 1)
    let after = world.target(origin: world.camera.position, direction: V3(0, -1, 0), maxReach: 6)
    XCTAssertEqual(after.hitKind, .ground)
    XCTAssertEqual(after.obstruction, .none)
    let beforeSupport = try XCTUnwrap(before.support), afterSupport = try XCTUnwrap(after.support)
    XCTAssertEqual(afterSupport.point.y - beforeSupport.point.y, 1, accuracy: 0.01)
    XCTAssertEqual(before.distance - after.distance, 1, accuracy: 0.02)
    XCTAssertEqual(try XCTUnwrap(after.support).point.y, world.groundHeight(x, z), accuracy: 0.001)
  }

  func testInvalidQueriesRemainFiniteAndOutOfWorldRayStopsAtBoundary() throws {
    let world = try meadowWorld()
    for reach: Float in [.nan, .infinity, -1, 0, 2.99, 16.01] {
      let target = world.cameraTarget(maxReach: reach)
      XCTAssertEqual(target.obstruction, .invalidInput)
      XCTAssertNil(target.support)
      XCTAssertTrue(target.point.x.isFinite && target.point.y.isFinite && target.point.z.isFinite)
      XCTAssertTrue(target.distance.isFinite && target.maxReach.isFinite)
    }
    for direction in [V3.zero, V3(.nan, 0, 1), V3(0, .infinity, 1)] {
      XCTAssertEqual(world.target(origin: world.camera.position, direction: direction, maxReach: 6).obstruction, .invalidInput)
    }
    XCTAssertEqual(world.target(origin: V3(.nan, 0, 0), direction: V3(0, -1, 0), maxReach: 6).point, .zero)
    let edge = world.target(origin: V3(15_999, 800, 0), direction: V3(1, 0, 0), maxReach: 6)
    XCTAssertEqual(edge.obstruction, .outsideWorld)
    XCTAssertEqual(edge.distance, 1)
    XCTAssertEqual(edge.point.x, 16_000)
  }
}
