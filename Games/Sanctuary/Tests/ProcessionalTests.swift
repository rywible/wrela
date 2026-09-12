import FieldCore
import XCTest
import simd

@testable import SanctuaryContent

final class ProcessionalTests: XCTestCase {
  func testEntirePerformanceContactsAndFiniteTransforms() throws {
    try PartRig.validate(ProcessionalMotion.joints)
    var maxReach: Float = 0
    var maxSlide: Float = 0
    var previous: ProcessionalMotion.Frame?
    for tick in 0...720 {
      let frame = ProcessionalMotion.frame(clip: "procession", time: Float(tick) / 60)
      maxReach = max(maxReach, frame.maximumReachError)
      let transforms = PartRig.matrices(ProcessionalMotion.joints, poses: frame.poses)
      for front in [true, false] {
        for side: Float in [-1, 1] {
          let id = ProcessionalMotion.limbID(front, side)
          let rest = ProcessionalMotion.restLimb(front, side)
          let upperEnd = transforms[id + "-upper"]! * SIMD4(rest.1, 1)
          let lowerStart = transforms[id + "-lower"]! * SIMD4(rest.1, 1)
          let lowerEnd = transforms[id + "-lower"]! * SIMD4(rest.2, 1)
          let foot = frame.feet[id]!
          XCTAssertLessThan(length(upperEnd - lowerStart), 0.0001)
          XCTAssertLessThan(length(lowerEnd - SIMD4(foot, 1)), 0.0001)
        }
      }
      for m in transforms.values {
        for c in 0..<4 { for r in 0..<4 { XCTAssertTrue(m[c][r].isFinite) } }
      }
      if let previous {
        for (id, foot) in frame.feet
        where frame.contacts[id] == true && previous.contacts[id] == true {
          maxSlide = max(maxSlide, length(foot - previous.feet[id]!))
        }
      }
      previous = frame
    }
    XCTAssertLessThan(maxReach, 0.002, "IK reach residual \(maxReach) m")
    XCTAssertLessThan(maxSlide, 0.002, "Planted contact moved \(maxSlide) m per frame")
  }
  func testJawOpensBelowTheMask() {
    func lowerLip(_ time:Float)->Float {
      let frame=ProcessionalMotion.frame(clip:"procession",time:time)
      let m=PartRig.matrices(ProcessionalMotion.joints,poses:frame.poses)
      let relative: simd_float4x4 = simd_mul(m["mask"]!.inverse, m["jaw"]!)
      let point: SIMD4<Float> = simd_mul(relative, SIMD4<Float>(0,2.7,-2.4,1))
      return point.y
    }
    XCTAssertLessThan(lowerLip(6.8),lowerLip(0)-0.2)
  }
  func testSeekIsDeterministic() {
    let a = ProcessionalMotion.frame(clip: "procession", time: 6.8)
    _ = ProcessionalMotion.frame(clip: "procession", time: 11.7)
    let b = ProcessionalMotion.frame(clip: "procession", time: 6.8)
    XCTAssertEqual(a.feet, b.feet)
  }
}
