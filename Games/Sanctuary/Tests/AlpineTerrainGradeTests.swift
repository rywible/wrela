import XCTest
@testable import SanctuaryContent
import simd

final class AlpineTerrainGradeTests: XCTestCase {
  func testRouteCorrectionIsBoundedAndVergeAddsNoCliff() {
    let mountain: Float = 500
    let mean: Float = 80
    for distance: Float in stride(from: 0, through: 190, by: 1) {
      let h = Terrain.routeGradedHeight(mountain, toward: mean, distance: distance)
      XCTAssertLessThanOrEqual(mountain - h, 16.401)
      XCTAssertGreaterThanOrEqual(mountain - h, 0)
      let before = Terrain.routeGradedHeight(mountain, toward: mean, distance: distance - 0.5)
      let after = Terrain.routeGradedHeight(mountain, toward: mean, distance: distance + 0.5)
      XCTAssertLessThanOrEqual(abs(after - before), 0.215)
    }
    XCTAssertEqual(Terrain.routeGradedHeight(mountain, toward: mean, distance: 150), mountain)
    XCTAssertEqual(Terrain.routeGradedHeight(mountain, toward: mean, distance: 800), mountain)
  }

  func testAlpineRidingApproachHasBoundedSourceGrade() {
    let terrain = Terrain()
    for x: Float in stride(from: -467, through: -387, by: 20) {
      for z: Float in stride(from: 10_575, through: 10_925, by: 5) {
        let dx = terrain.height(x + 0.5, z) - terrain.height(x - 0.5, z)
        let dz = terrain.height(x, z + 0.5) - terrain.height(x, z - 0.5)
        // Preserve the mountain's cross-slope while keeping the riding direction below 29°.
        XCTAssertLessThan(abs(dz), 0.55, "Riding grade at \(x), \(z)")
        XCTAssertLessThan(hypot(dx, dz), 0.75, "Combined mountain grade at \(x), \(z)")
      }
    }
  }

  func testOrdinaryRidingEyeCanSeeSkyAtBothStreamingCaptures() {
    let terrain = Terrain()
    let x: Float = -427
    // Same world-space positions and ordinary pitch/FOV as the failing native route.
    // Production riding grounding samples a quarter metre ahead before adding 2.5 m.
    for z: Float in [10_745.171875, 10_754.1953125] {
      let eye = terrain.height(x, z + 0.25) + 2.5
      let topSlope = tan(Float.pi / 6 - 0.08)
      for distance: Float in stride(from: 0.25, through: 2_000, by: 2) {
        XCTAssertGreaterThan(eye + topSlope * distance - terrain.height(x, z + distance), 0.5,
          "Top view is blocked at z=\(z), distance=\(distance)")
      }
    }
  }

  func testRouteRepairPreservesAuthoredSummitElevations() {
    let terrain = Terrain()
    let center = SanctuaryGeography().landmark(for: .alpine).coordinate
    let summits: [(SIMD2<Float>, Float)] = [
      (SIMD2(720, 430), 800), (SIMD2(-1_050, 820), 575), (SIMD2(1_620, -720), 550),
    ]
    for (offset, minimumHeight) in summits {
      let p = center + offset
      XCTAssertGreaterThan(terrain.height(p.x, p.y), minimumHeight)
    }
  }
}
