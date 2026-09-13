import FieldCore
import simd
import XCTest
@testable import SoundstageKit

final class StudioProjectionTests: XCTestCase {
  func testFourMetreCoverageStudyResolvesSixMillimetreLayer() {
    let extent: Float = 0.164
    let distance: Float = 4
    let radius = StudioProjection.conservativeRadius(
      minimum: V3(-0.082, 0, -0.005057), maximum: V3(0.082, 0.112, 0.005057),
      around: V3(0, 0.056, 0))
    let near = StudioProjection.nearPlane(
      cameraDistance: distance, subjectRadius: radius, subjectExtent: extent)
    XCTAssertEqual(near, 0.00164, accuracy: 0.0000001)

    // Forward float depth near one has roughly d^2 / near * 2^-24
    // metres per stored value. The old 0.1%-extent near plane left the
    // authored 6 mm gap only one value wide at this camera distance.
    let floatDepthUnit: Float = 1 / 16_777_216
    let step = distance * distance / near * floatDepthUnit
    XCTAssertGreaterThan(0.006 / step, 10)
    let oldStep = distance * distance / (extent * 0.001) * floatDepthUnit
    XCTAssertLessThan(0.006 / oldStep, 1.1)
  }

  func testCloseSeedPodKeepsNearPlaneBeforeConservativeSurface() {
    let extent: Float = 0.035
    let distance: Float = 0.04
    let radius: Float = 0.021
    let near = StudioProjection.nearPlane(
      cameraDistance: distance, subjectRadius: radius, subjectExtent: extent)
    XCTAssertEqual(near, 0.00035, accuracy: 0.0000001)
    XCTAssertLessThan(near, distance - radius)
  }

  func testCameraInsideBoundRetainsOriginalCloseInspectionNearPlane() {
    let near = StudioProjection.nearPlane(
      cameraDistance: 0.01, subjectRadius: 0.02, subjectExtent: 0.035)
    XCTAssertEqual(near, 0.000035, accuracy: 0.0000001)
  }

  func testTightExteriorClearanceClampsWithoutCrossingBound() {
    let distance: Float = 0.0201
    let radius: Float = 0.02
    let near = StudioProjection.nearPlane(
      cameraDistance: distance, subjectRadius: radius, subjectExtent: 0.035)
    XCTAssertEqual(near, 0.00005, accuracy: 0.0000001)
    XCTAssertLessThan(near, distance - radius)
  }

  func testConservativeRadiusContainsEveryBoundsCorner() {
    let minimum = V3(-0.08, -0.01, -0.02)
    let maximum = V3(0.05, 0.12, 0.03)
    let focus = V3(0.01, 0.04, -0.005)
    let radius = StudioProjection.conservativeRadius(
      minimum: minimum, maximum: maximum, around: focus)
    for x in [minimum.x, maximum.x] {
      for y in [minimum.y, maximum.y] {
        for z in [minimum.z, maximum.z] {
          XCTAssertLessThanOrEqual(length(V3(x, y, z) - focus), radius + 0.000001)
        }
      }
    }
  }
}
