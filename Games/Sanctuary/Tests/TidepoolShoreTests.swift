import Foundation
import XCTest
import simd

@testable import SanctuaryContent

final class TidepoolShoreTests: XCTestCase {
  func testProductionTidepoolRouteStaysDryOrShallow() throws {
    let terrain = Terrain()
    for z in stride(from: Float(-9_500), through: -9_520, by: -1) {
      let point = SIMD2<Float>(12_976, z)
      let ground = terrain.height(point.x, point.y)
      XCTAssertGreaterThan(ground, Terrain.oceanSurfaceHeight, "\(point)")
      if let water = terrain.water(at: point) {
        XCTAssertEqual(water.body, .tidepool, "\(point)")
        XCTAssertEqual(water.depth, 0.34, accuracy: 0.0001, "\(point)")
        XCTAssertLessThanOrEqual(water.surfaceHeight - ground, 0.5, "\(point)")
      }
      XCTAssertTrue(try terrain.surface(at: point).isWalkableSurface, "\(point)")
    }
  }

  func testTidepoolerHomesHaveReachableSurface() throws {
    let terrain = Terrain()
    let population = WildlifePopulation.initial()
    for id in ["tidepooler-001", "tidepooler-002"] {
      let home = try XCTUnwrap(population.actor(id: id)).position
      let ground = terrain.height(home.x, home.y)
      XCTAssertGreaterThan(ground, Terrain.oceanSurfaceHeight, id)
      XCTAssertNotEqual(terrain.water(at: home)?.body, .ocean, id)
      XCTAssertTrue(try terrain.surface(at: home).isWalkableSurface, id)
    }
  }

  func testRaisedShelfCrossesSeaLevelContinuouslyIntoDeepOcean() throws {
    let terrain = Terrain()
    let center = SanctuaryGeography().landmark(for: .tidepool).coordinate
    XCTAssertGreaterThan(
      terrain.height(center.x, center.y) - Terrain.oceanSurfaceHeight, 1.0)

    var previous = terrain.height(center.x, center.y)
    for distance: Float in stride(from: 10, through: 2_300, by: 10) {
      let height = terrain.height(center.x, center.y - distance)
      XCTAssertLessThan(abs(height - previous), 1.5, "shore discontinuity at \(distance)m")
      previous = height
    }

    let offshore = center + SIMD2<Float>(0, -2_300)
    let water = try XCTUnwrap(terrain.water(at: offshore))
    XCTAssertEqual(water.body, .ocean)
    XCTAssertLessThan(terrain.height(offshore.x, offshore.y), Terrain.oceanSurfaceHeight - 5)
    XCTAssertEqual(
      water.depth, water.surfaceHeight - terrain.height(offshore.x, offshore.y), accuracy: 0.0001)
  }

  func testShelfJoinsAreContinuousInEightDirections() {
    let terrain = Terrain()
    let center = SanctuaryGeography().landmark(for: .tidepool).coordinate
    let radii = SIMD2<Float>(1_250, 1_900)
    for index in 0..<8 {
      let angle = Float(index) * .pi / 4
      let direction = SIMD2<Float>(cos(angle), sin(angle))
      func point(_ radius: Float) -> SIMD2<Float> {
        center + direction * radii * radius
      }
      for boundary: Float in [0.98, 1.18] {
        let inside = point(boundary - 0.001)
        let outside = point(boundary + 0.001)
        let delta = abs(
          terrain.height(inside.x, inside.y) - terrain.height(outside.x, outside.y))
        XCTAssertLessThan(
          delta, 0.5, "join \(boundary), direction \(index), delta \(delta)")
      }
    }
  }

  func testShallowPoolsSitAboveTheOceanRatherThanUnderIt() throws {
    let terrain = Terrain()
    let center = SanctuaryGeography().landmark(for: .tidepool).coordinate
    let pool = center + SIMD2<Float>(60, 0)
    let water = try XCTUnwrap(terrain.water(at: pool))
    let ground = terrain.height(pool.x, pool.y)
    XCTAssertEqual(water.body, .tidepool)
    XCTAssertEqual(water.depth, 0.34, accuracy: 0.0001)
    XCTAssertGreaterThan(ground, Terrain.oceanSurfaceHeight)
    XCTAssertEqual(water.surfaceHeight, ground + 0.34, accuracy: 0.0001)
  }
}
