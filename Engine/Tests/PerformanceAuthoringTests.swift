import FieldCompiler
import FieldCore
import XCTest
import simd

final class PerformanceAuthoringTests: XCTestCase {
  func testCurveHoldsAndRejectsDuplicateTimes() throws {
    let curve = MotionCurve([MotionKey(0, 0), MotionKey(1, 20), MotionKey(2, 20)])
    XCTAssertEqual(curve.value(at: 0.5), 10, accuracy: 0.0001)
    XCTAssertEqual(curve.value(at: 1.8), 20)
    XCTAssertThrowsError(try MotionCurve([MotionKey(0, 0), MotionKey(0, 1)]).validate(duration: 2))
  }
  func testScoreRoundTripAndRejection() throws {
    let score = PerformanceScore(
      clip: "dance", duration: 2,
      tracks: [PoseTrack("jaw", "pitch", [MotionKey(0, 0), MotionKey(1, 20), MotionKey(2, 0)])])
    try score.validate(joints: ["jaw"])
    let data = try JSONEncoder().encode(score)
    XCTAssertEqual(try JSONDecoder().decode(PerformanceScore.self, from: data), score)
    XCTAssertEqual(score.apply(to: [:], time: 0.5)["jaw"]!.rotation.x, 10, accuracy: 0.0001)
    XCTAssertThrowsError(try score.validate(joints: ["head"]))
  }
  func testSkinBindPoseAndTranslation() {
    let weight = SkinWeight(0, 1, blend: 0.25)
    XCTAssertTrue(weight.validate(count: 2))
    XCTAssertFalse(weight.validate(count: 1))
    XCTAssertEqual(MemoryLayout<SkinWeight>.stride, 32)
    var moved = matrix_identity_float4x4
    moved[3] = SIMD4(4, 0, 0, 1)
    let p = weight.matrix([matrix_identity_float4x4, moved]) * SIMD4<Float>(0, 0, 0, 1)
    XCTAssertEqual(p, SIMD4(1, 0, 0, 1))
  }
  func testReachLengthsAndUnreachableResidual() {
    let result = TwoBoneIK.solve(
      root: .zero, target: V3(1, 1, 0), pole: V3(0, 0, 1), upper: 1, lower: 1)
    XCTAssertEqual(length(result.joint), 1, accuracy: 0.0001)
    XCTAssertEqual(length(result.end - result.joint), 1, accuracy: 0.0001)
    XCTAssertLessThan(result.residual, 0.0001)
    let distant = TwoBoneIK.solve(
      root: .zero, target: V3(5, 0, 0), pole: V3(0, 1, 0), upper: 1, lower: 1)
    XCTAssertGreaterThan(distant.residual, 2.9)
  }
  func testParametricSurfaceWinding() {
    let mesh = ParametricMesh.surface(u: 2, v: 2) { u, v in V3(u, 0, -v) }
    XCTAssertEqual(mesh.indices.count, 24)
    for vertex in mesh.vertices { XCTAssertGreaterThan(vertex.normal.y, 0.99) }
    let a = mesh.vertices[Int(mesh.indices[0])].position
    let b = mesh.vertices[Int(mesh.indices[1])].position
    let c = mesh.vertices[Int(mesh.indices[2])].position
    XCTAssertGreaterThan(
      cross(V3(b.x - a.x, b.y - a.y, b.z - a.z), V3(c.x - a.x, c.y - a.y, c.z - a.z)).y, 0)
  }
}
