import FieldEngine
import XCTest
import simd

final class CameraMathTests: XCTestCase {
  func testGameProjectionClampsFarDistanceAndKeepsNearPlane() {
    let projection = FrameComposer.cameraProjection(100_000)
    let near = projection * SIMD4<Float>(0, 0, -0.08, 1)
    let far = projection * SIMD4<Float>(0, 0, -50_000, 1)
    XCTAssertEqual(near.z / near.w, 0, accuracy: 0.00001)
    XCTAssertEqual(far.z / far.w, 1, accuracy: 0.00001)
    XCTAssertEqual(FrameComposer.validatedVisibilityDistance(.infinity), 450)
  }
}
