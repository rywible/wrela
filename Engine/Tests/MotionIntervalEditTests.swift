import FieldCore
import XCTest

final class MotionIntervalEditTests: XCTestCase {
  func source() -> MotionPhrase {
    MotionPhrase(id: "body-study", duration: 4,
      samples: (0...4).map { i in
        PoseSample(time: Float(i), poses: [
          "trunk": JointPose(offset: V3(0, i == 2 ? -0.2 : 0, 0)),
          "gaze": JointPose(rotation: V3(0, Float(i) * 10, 0)),
        ])
      }, contacts: [ContactPath(chain: "support", keys: [
        ContactKey(time: 0, position: .zero, planted: true),
        ContactKey(time: 4, position: .zero, planted: true),
      ])], interpolation: .spline)
  }

  func testCrouchAndDelayedGazeKeepContactsAndProtectedLanding() throws {
    let original = source()
    let edited = try MotionIntervalEdit(start: 0.5, end: 3.5, fadeIn: 0.7, fadeOut: 0.7,
      adjustments: [MotionIntervalAdjustment(joint: "trunk", offset: V3(0, -0.3, 0)),
                    MotionIntervalAdjustment(joint: "gaze", delay: 0.15)],
      bounds: [PoseFreedom(joint: "trunk", channel: "y", minimum: -0.6, maximum: 0.1)],
      protectedTimes: [3]).applying(to: original)
    XCTAssertEqual(edited.contacts, original.contacts)
    XCTAssertEqual(edited.pose(at: 3), original.pose(at: 3))
    XCTAssertEqual(edited.samples.first, original.samples.first)
    XCTAssertEqual(edited.samples.last, original.samples.last)
    XCTAssertLessThan(edited.pose(at: 2)["trunk"]!.offset.y, -0.45)
    XCTAssertLessThan(edited.pose(at: 2)["gaze"]!.rotation.y, original.pose(at: 2)["gaze"]!.rotation.y)
  }

  func testConstraintFailuresLeaveInputUnchanged() throws {
    let original = source(), saved = original
    XCTAssertThrowsError(try MotionIntervalEdit(start: 0.5, end: 3.5, fadeIn: 0.1, fadeOut: 0.1,
      adjustments: [MotionIntervalAdjustment(joint: "gaze", delay: 1)]).applying(to: original)) {
        XCTAssertTrue(String(describing: $0).contains("local clock"))
    }
    XCTAssertThrowsError(try MotionIntervalEdit(start: 0.5, end: 3.5, fadeIn: 0.7, fadeOut: 0.7,
      adjustments: [MotionIntervalAdjustment(joint: "trunk", offset: V3(0, -0.5, 0))],
      bounds: [PoseFreedom(joint: "trunk", channel: "y", minimum: -0.3, maximum: 0.1)]).applying(to: original)) {
        XCTAssertTrue(String(describing: $0).contains("trunk.y"))
    }
    XCTAssertEqual(original, saved)
  }

  func testUnrecognizedJointAndInvalidFadesReject() {
    XCTAssertThrowsError(try MotionIntervalEdit(start: 0.5, end: 3.5, fadeIn: 1, fadeOut: 1,
      adjustments: [MotionIntervalAdjustment(joint: "human-head")]).applying(to: source()))
    XCTAssertThrowsError(try MotionIntervalEdit(start: 0.5, end: 3.5, fadeIn: 0, fadeOut: 1,
      adjustments: [MotionIntervalAdjustment(joint: "gaze")]).applying(to: source()))
  }
}
