import FieldCore
import XCTest
@testable import SanctuaryContent
import simd

final class BotanicalPlacementTests: XCTestCase {
  func testCapsuleChainsCoverReviewedTrunkSurfacesWithFiniteBounds() throws {
    for form: SanctuaryBotanicalTrunk.Form in [.willow, .palm, .conifer] {
      let trunk = SanctuaryBotanicalTrunk(form)
      let collision = trunk.collisionShape()
      try collision.validate()
      for component in 0..<3 {
        XCTAssertTrue(collision.bounds.min[component].isFinite)
        XCTAssertTrue(collision.bounds.max[component].isFinite)
      }
      XCTAssertLessThan(collision.bounds.max.y, form == .conifer ? 8.1 : 6)
      for index in 0...128 {
        let t = Float(index) / 128
        let center = trunk.point(at: t)
        let tangent = normalize(trunk.point(at: min(1, t + 0.001))
          - trunk.point(at: max(0, t - 0.001)))
        let across = normalize(cross(tangent, V3(1, 0, 0)))
        let side = cross(tangent, across)
        for angleIndex in 0..<16 {
          let angle = Float(angleIndex) * 2 * .pi / 16
          let surface = center + (across * cos(angle) + side * sin(angle)) * trunk.radius(at: t)
          XCTAssertLessThanOrEqual(collision.value(at: surface), 0.0001)
        }
        XCTAssertGreaterThan(collision.value(at: center + V3(2, 0, 0)), 0.5)
      }
    }
  }

  func testExistingBotanicalRecordsKeepPlacementAndShareCollisionTransform() throws {
    let geography = SanctuaryGeography()
    let terrain = Terrain()
    let cases: [(SanctuaryBiome, SanctuaryFeatureKind, SanctuaryBotanicalTrunk.Form)] = [
      (.creek, .willow, .willow), (.rainforest, .palm, .palm),
      (.alpine, .conifer, .conifer),
    ]
    for (biome, kind, form) in cases {
      let center = geography.landmark(for: biome).coordinate
      let plans = SanctuaryTerrainChunkKey.neighborhood(around: center).map { geography.plan(for: $0) }
      let records = plans.flatMap(\.features).filter { $0.kind == kind }
      var resolvedCount = 0
      for record in records {
        guard let feature = SanctuaryLayout.resolveFeature(record, terrain: terrain) else { continue }
        resolvedCount += 1
        XCTAssertEqual(feature.source, record)
        XCTAssertEqual(feature.position.x, record.coordinate.x)
        XCTAssertEqual(feature.position.z, record.coordinate.y)
        XCTAssertEqual(feature.yaw, record.yaw)
        XCTAssertEqual(feature.coreScale, V3(repeating: record.scale))
        XCTAssertEqual(feature.canopyScale, feature.coreScale)
        let collision = try XCTUnwrap(SanctuaryLayout.collisionShape(for: feature))
        try collision.validate()
        let rotation = simd_quatf(angle: record.yaw, axis: V3(0, 1, 0))
        let trunk = SanctuaryBotanicalTrunk(form)
        for t: Float in [0, 0.25, 0.5, 0.75, 1] {
          let point = rotation.act(trunk.point(at: t) * record.scale)
          XCTAssertLessThan(collision.value(at: point), 0)
        }
      }
      XCTAssertGreaterThan(resolvedCount, 0)
      for plan in plans { XCTAssertEqual(plan, geography.plan(for: plan.key)) }
    }
  }
}
