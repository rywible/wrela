import FieldCore
import FieldEngine
@testable import SanctuaryContent
import XCTest
import simd
@testable import SanctuaryProject

final class NaturePreviewSupportTests: XCTestCase {
  private typealias Cache = SanctuaryNaturePreviewPresentation.SupportCache

  /// Deform the exact game boundary source with the exact production cache.
  private func boundaryPoints(_ cache: Cache, at point: V3) -> [V3] {
    let source = SanctuaryNaturePreviewPresentation.boundaryRecipe
    return source.mesh.vertices.indices.map { index in
      let p = source.skinWeights[index].matrix(cache.palette) * source.mesh.vertices[index].position
      return point + V3(p.x, p.y, p.z)
    }
  }

  func testDryGroundExtremesKeepTheExistingContourAndReuseSupport() throws {
    let world = try SanctuaryWorld(seed: 17)
    let point = V3(10, world.groundHeight(10, 18), 18)
    for radius: Float in [1, 16] {
      var terrain = Cache(), visible = Cache()
      terrain.update(point: point, radius: radius, revision: 1, supportHeight: world.groundHeight)
      visible.update(point: point, radius: radius, revision: 1, supportHeight: world.flightSurfaceHeight)
      XCTAssertEqual(visible.queriesLastFrame, 61)
      XCTAssertEqual(visible.markerPoint, point)
      let oldPoints = boundaryPoints(terrain, at: point), newPoints = boundaryPoints(visible, at: point)
      XCTAssertEqual(oldPoints, newPoints, "Dry contours must not change with visible-water support")
      var repeatedQueries = 0
      visible.update(point: point, radius: radius, revision: 1) { x, z in
        repeatedQueries += 1
        return world.flightSurfaceHeight(x, z)
      }
      XCTAssertEqual(repeatedQueries, 0)
      XCTAssertEqual(visible.queriesLastFrame, 0)
      visible.reset()
      XCTAssertEqual(visible.queriesLastFrame, 0)
      visible.update(point: point, radius: radius, revision: 1, supportHeight: world.flightSurfaceHeight)
      XCTAssertEqual(visible.queriesLastFrame, 61)
    }
  }

  func testPublishedWaterRaisesBothContoursAndCenterAtAStationaryTarget() throws {
    let world = try SanctuaryWorld(seed: 17)
    world.enableHostCommittedRegionalCollision(initialLayout: world.world, garden: nil,
      boulders: world.controller.state.boulders)
    let point = V3(10, world.groundHeight(10, 18), 18)
    var cache = Cache()
    cache.update(point: point, radius: 1, revision: 1, supportHeight: world.flightSurfaceHeight)
    let before = boundaryPoints(cache, at: point)
    _ = try world.controller.applyNature(.plant(.shallowWater, at: .init(x: 10, z: 18), radius: 4),
      expectedRevision: 0)
    // Saved water must not move the guide before the host publishes that surface.
    cache.update(point: point, radius: 1, revision: 1, supportHeight: world.flightSurfaceHeight)
    XCTAssertEqual(cache.queriesLastFrame, 0)
    XCTAssertEqual(cache.markerPoint, point)
    XCTAssertTrue(world.publishRegionalState(generation: 0, layout: world.world,
      garden: world.controller.state.garden, boulders: world.controller.state.boulders))
    cache.update(point: point, radius: 1, revision: 2, supportHeight: world.flightSurfaceHeight)
    let water = try XCTUnwrap(world.localWaterHeight(point.x, point.z))
    XCTAssertGreaterThan(water - point.y, 0.1, "Exercise the actual submerged-guide failure")
    XCTAssertEqual(cache.markerPoint.y, water, accuracy: 0.00001)
    XCTAssertEqual(cache.queriesLastFrame, 61)
    let after = boundaryPoints(cache, at: point)
    XCTAssertTrue(zip(before, after).allSatisfy { $0.1.y > $0.0.y + 0.05 })
    for vertex in after {
      XCTAssertGreaterThan(vertex.y, world.flightSurfaceHeight(vertex.x, vertex.z) + 0.02)
    }
    cache.update(point: point, radius: 1, revision: 2, supportHeight: world.flightSurfaceHeight)
    XCTAssertEqual(cache.queriesLastFrame, 0)
  }

  func testMixedShorelineUsesPublishedWaterAndDryBankWithinTheSameBoundary() throws {
    let world = try SanctuaryWorld(seed: 17)
    _ = try world.controller.applyNature(.plant(.shallowWater, at: .init(x: 10, z: 18), radius: 4),
      expectedRevision: 0)
    let point = V3(12, world.groundHeight(12, 18), 18)
    var cache = Cache()
    cache.update(point: point, radius: 4, revision: 1, supportHeight: world.flightSurfaceHeight)
    let vertices = boundaryPoints(cache, at: point)
    var wet = 0, dry = 0
    for vertex in vertices {
      let ground = world.groundHeight(vertex.x, vertex.z)
      if let water = world.localWaterHeight(vertex.x, vertex.z), water > ground {
        wet += 1
        XCTAssertGreaterThan(vertex.y, water + 0.02, "Wet boundary must remain above visible water")
      } else {
        dry += 1
        XCTAssertGreaterThan(vertex.y, ground + 0.02, "Dry-bank boundary retains its ground clearance")
      }
      XCTAssertTrue(vertex.x.isFinite && vertex.y.isFinite && vertex.z.isFinite)
    }
    XCTAssertGreaterThan(wet, 0)
    XCTAssertGreaterThan(dry, 0)
    XCTAssertEqual(cache.queriesLastFrame, 61)
    XCTAssertEqual(SanctuaryNaturePreviewPresentation.boundaryRecipe.mesh.indices.count / 3, 480)
  }

  func testEcologyWaterChangesTheProductionKeyAtTheSameTargetAndSettles() {
    typealias Key = SanctuaryExperience.NaturePresentationSupportKey
    let garden = SanctuaryGardenSurfaceSource(garden: nil)
    let dry = Key(garden: garden, ecologicalWater: [])
    let dam = EcologyWaterFact(structureID: "brookweaver-001-dam-001", center: SIMD2(10, 18),
      radius: 2.2, upstreamPoolRadius: 6.6, waterLevelRise: 0.28)
    let flooded = Key(garden: garden, ecologicalWater: [dam])
    XCTAssertNotEqual(dry, flooded, "A dam pool must invalidate support without a garden edit")
    XCTAssertEqual(flooded, Key(garden: garden, ecologicalWater: [dam]))

    // Constant support isolates cache invalidation; the tests above exercise
    // actual world water/ground sampling and publication. Use the production
    // key comparison to advance exactly the revision consumed by the renderer.
    let point = V3(10, 0, 18)
    var cache = Cache(), revision: UInt64 = 1
    cache.update(point: point, radius: 1, revision: revision) { _, _ in 0 }
    let before = boundaryPoints(cache, at: point)
    if flooded != dry { revision += 1 }
    cache.update(point: point, radius: 1, revision: revision) { _, _ in dam.waterLevelRise }
    XCTAssertEqual(cache.queriesLastFrame, 61)
    XCTAssertEqual(cache.markerPoint.y, dam.waterLevelRise, accuracy: 0.00001)
    let after = boundaryPoints(cache, at: point)
    for (a, b) in zip(before, after) {
      XCTAssertEqual(b.y - a.y, dam.waterLevelRise, accuracy: 0.00001)
    }
    var repeatedQueries = 0
    cache.update(point: point, radius: 1, revision: revision) { _, _ in
      repeatedQueries += 1
      return dam.waterLevelRise
    }
    XCTAssertEqual(repeatedQueries, 0)
    XCTAssertEqual(cache.queriesLastFrame, 0)
  }
}
