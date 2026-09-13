import XCTest
@testable import FieldEngine

final class SceneLODSelectionTests: XCTestCase {
  private func select(
    current: Int = 0, errors: [Float], scale: Float = 1, distance: Float,
    kind: Float = 0, top: Float = 0, wind: Float = 0, shadow: Bool = false,
    shadowTexelsPerMetre: Float = 1
  ) -> Int {
    SceneLODSelection.selectedLevel(
      currentLOD: current, errors: errors, scale: scale, distance: distance,
      kind: kind, top: top, wind: wind, shadow: shadow,
      shadowTexelsPerMetre: shadowTexelsPerMetre)
  }

  func testProjectedErrorUsesStrictTransitionBoundaryAndScale() {
    XCTAssertEqual(select(errors: [], distance: 1), 0)
    XCTAssertEqual(select(errors: [0.04], distance: 93), 0)
    XCTAssertEqual(select(errors: [0.04], distance: 94), 1)
    XCTAssertEqual(select(errors: [0.04], scale: 2, distance: 94), 0)
    XCTAssertEqual(select(errors: [0.04], scale: 2, distance: 188), 1)
  }

  func testCurrentLevelGetsTheExistingHysteresisThreshold() {
    // 0.04 * 935 / 70 is between the entering 0.4 threshold and retained 0.65 threshold.
    XCTAssertEqual(select(current: 0, errors: [0.04], distance: 70), 0)
    XCTAssertEqual(select(current: 1, errors: [0.04], distance: 70), 1)
    XCTAssertEqual(select(current: 1, errors: [0.04], distance: 50), 0)
  }

  func testFoliageWindErrorCanKeepTheDetailedLevel() {
    XCTAssertEqual(select(errors: [0.025], distance: 70, kind: 8, top: 8, wind: 0), 1)
    XCTAssertEqual(select(errors: [0.025], distance: 70, kind: 8, top: 8, wind: 1), 0)
    XCTAssertEqual(select(errors: [0.025], distance: 70, kind: 2, top: 8, wind: 1), 1)
  }

  func testShadowSelectionUsesTexelErrorAndIgnoresCameraHysteresis() {
    XCTAssertEqual(select(current: 1, errors: [0.40], distance: 10_000,
      shadow: true, shadowTexelsPerMetre: 2), 0)
    XCTAssertEqual(select(current: 0, errors: [0.39], distance: 1,
      shadow: true, shadowTexelsPerMetre: 2), 1)
    XCTAssertEqual(select(current: 1, errors: [0.39], distance: 100,
      shadow: true, shadowTexelsPerMetre: 2), 1)
  }

  func testMultipleLevelsSelectTheLastLevelWithinTheSameProductionBudget() {
    XCTAssertEqual(select(errors: [0.02, 0.08], distance: 100), 1)
    XCTAssertEqual(select(errors: [0.02, 0.08], distance: 200), 2)
  }
}
