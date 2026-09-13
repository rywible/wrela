import XCTest
import simd

@testable import SanctuaryContent

final class MoonlakeShoreDiscoveryTests: XCTestCase {
  func testMoonhartHomeIsDryReachableMoonlakeShore() throws {
    let geography = SanctuaryGeography()
    let terrain = Terrain()
    let lake = geography.landmark(for: .lake)
    let shore = try XCTUnwrap(WildlifePopulation.initial().actor(id: "moonhart-002")).position

    XCTAssertEqual(lake.coordinate, SIMD2<Float>(6_020, 6_300))
    XCTAssertEqual(shore, SIMD2<Float>(7_085, 6_300))
    XCTAssertNil(terrain.water(at: shore))
    XCTAssertTrue(geography.isAtLandmark(lake, position: shore))
  }

  func testDeepCenterAndDryShoreDiscoverTheSameStableLandmark() throws {
    let geography = SanctuaryGeography()
    let terrain = Terrain()
    let lake = geography.landmark(for: .lake)
    let centerWater = try XCTUnwrap(terrain.water(at: lake.coordinate))
    XCTAssertEqual(centerWater.body, .lake)
    XCTAssertGreaterThan(centerWater.surfaceHeight, terrain.height(lake.coordinate.x, lake.coordinate.y))

    var journey = SanctuaryJourney()
    journey.discover(at: SIMD2<Float>(7_085, 6_300))
    XCTAssertEqual(journey.discoveredLandmarks, ["moonlake"])

    let restored = try JSONDecoder().decode(
      SanctuaryJourney.self, from: JSONEncoder().encode(journey))
    XCTAssertEqual(restored, journey)
    XCTAssertEqual(SanctuaryGeography.bounds.size, SIMD2<Float>(repeating: 32_000))
  }

  func testShoreBandTracksSharedWaterFootprint() {
    let geography = SanctuaryGeography()
    let lake = geography.landmark(for: .lake)
    let footprint = SanctuaryGeography.moonlakeFootprint
    let wet = footprint.center + SIMD2<Float>(footprint.radii.x * 0.999, 0)
    let shore = footprint.center + SIMD2<Float>(footprint.radii.x * 1.03, 0)
    let farBank = footprint.center + SIMD2<Float>(footprint.radii.x * 1.061, 0)

    XCTAssertTrue(footprint.contains(wet))
    XCTAssertFalse(geography.isAtLandmark(lake, position: wet))
    XCTAssertTrue(geography.isAtLandmark(lake, position: shore))
    XCTAssertFalse(geography.isAtLandmark(lake, position: farBank))
  }
}
