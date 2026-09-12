import FieldCore
import Foundation
import simd

/// Vesper's anatomy and choreography are shared source, independent of renderer/editor.
public enum ProcessionalMotion {
  public static let duration: Float = 12
  public static let beats = [
    PerformanceBeat("Attend", 0, "The mask listens. Feet and body remain quiet."),
    PerformanceBeat(
      "Gather", 1.8, "Sink into the planted feet; the mask turns before the shoulders."),
    PerformanceBeat(
      "Ascend", 3.5, "Unfurl the long spine; lift one paw in a measured ceremonial step."),
    PerformanceBeat("Coil", 5.2, "Counter-rotate the haunch and mask. Hold the silhouette."),
    PerformanceBeat(
      "Sweep", 6.5, "Lead with the mask, then release the spine into a broad lateral strike."),
    PerformanceBeat("Catch", 7.8, "Catch the weight with the forward paws and lower the head."),
    PerformanceBeat(
      "Recover", 9.2, "Let the mantle settle as the mask regains its formal bearing."),
    PerformanceBeat("Still", 11, "Return to the opening stance without a loop discontinuity."),
  ]
  public static var joints: [PartJoint] {
    var j = [
      PartJoint(id: "body", pivot: V3(0, 1.7, 0.5)),
      PartJoint(id: "haunch", parent: "body", pivot: V3(0, 1.55, 1.55)),
      PartJoint(id: "neck", parent: "body", pivot: V3(0, 2.3, -0.9)),
      PartJoint(id: "mask", parent: "neck", pivot: V3(0, 3, -1.65)),
      PartJoint(id: "jaw", parent: "mask", pivot: V3(0, 2.85, -1.75)),
      PartJoint(id: "mane", parent: "mask", pivot: V3(0, 3.1, -1.4)),
      PartJoint(id: "horns", parent: "mask", pivot: V3(0, 3.5, -1.5)),
      PartJoint(id: "eyes", parent: "mask", pivot: V3(0, 3.1, -1.8)),
      PartJoint(id: "mantle", parent: "body", pivot: V3(0, 2, 0.4)),
      PartJoint(id: "trim", parent: "body", pivot: V3(0, 2, 0.4)),
      PartJoint(id: "tail", parent: "haunch", pivot: V3(0, 1.6, 2)),
      PartJoint(id: "beard", parent: "jaw", pivot: V3(0, 2.55, -2.1)),
    ]
    for front in [true, false] {
      for side: Float in [-1, 1] {
        let id = limbID(front, side)
        let points = restLimb(front, side)
        j.append(PartJoint(id: id + "-upper", pivot: points.0))
        j.append(PartJoint(id: id + "-lower", pivot: points.1))
        j.append(PartJoint(id: id + "-paw", pivot: points.2))
      }
    }
    return j
  }
  public static func limbID(_ front: Bool, _ side: Float) -> String {
    (front ? "fore" : "hind") + (side < 0 ? "-left" : "-right")
  }
  public static func restLimb(_ front: Bool, _ side: Float) -> (V3, V3, V3) {
    if front {
      return (
        V3(side * 0.64, 2.25, -0.85), V3(side * 0.82, 1.18, 0.05), V3(side * 0.93, 0.18, -1.3)
      )
    }
    return (V3(side * 0.58, 1.5, 1.55), V3(side * 0.82, 0.84, 2.2), V3(side * 0.87, 0.18, 1.45))
  }
  static func curve(_ time: Float, _ pairs: [(Float, Float)]) -> Float {
    MotionCurve(pairs.map { MotionKey($0.0, $0.1) }).value(at: time)
  }
  public struct Frame {
    public var poses: [String: JointPose]
    public var targets: [String: V3]
    public var feet: [String: V3]
    public var contacts: [String: Bool]
    public var maximumReachError: Float
    public var beat: String
  }
  public static func frame(
    clip: String, time: Float, amplitude: Float = 1,
    tempo: Float = 1, followThrough: Float = 1
  ) -> Frame {
    let t = max(0, time * tempo).truncatingRemainder(dividingBy: duration)
    let dancing = clip != "idle"
    let s: Float = dancing ? amplitude : 0
    let rise =
      curve(
        t,
        [
          (0, 0), (1.8, 0), (3.0, -0.23), (4.6, 0.72), (5.4, 0.64), (6.5, 0.42), (7.6, -0.23),
          (8.5, -0.17), (10.8, 0), (12, 0),
        ]) * s
    let turn =
      curve(
        t,
        [
          (0, 0), (1.8, 0), (3.5, -12), (5.4, -36), (6.3, -48), (7.3, 59), (8.3, 24), (10.8, 0),
          (12, 0),
        ]) * s
    let lean =
      curve(
        t,
        [(0, 0), (2, 0), (3.2, 9), (4.8, -13), (6, -8), (7.35, 27), (8.2, 15), (10.8, 0), (12, 0)])
      * s
    let sway =
      curve(
        t, [(0, 0), (2, 0), (4, -0.16), (6, -0.24), (7.4, 0.3), (8.7, 0.14), (10.8, 0), (12, 0)])
      * s
    let breath = sin(time * 1.6) * 0.018
    var poses: [String: JointPose] = [
      "body": JointPose(
        offset: V3(sway, breath, 0), rotation: V3(-lean * 0.24, turn * 0.35, -sway * 9)),
      "neck": JointPose(offset: V3(0, rise, 0), rotation: V3(-lean, turn * 0.55, sway * 11)),
      "haunch": JointPose(rotation: V3(lean * 0.3, -turn * 0.25, sway * 8)),
      "mask": JointPose(
        rotation: V3(
          lean * 0.65,
          curve(
            t,
            [
              (0, 0), (1.2, 0), (2.8, -18), (5.2, -9), (6, -20), (6.9, 30), (8, 4), (10.5, 0),
              (12, 0),
            ]) * s, -turn * 0.18)),
      "jaw": JointPose(
        rotation: V3(
          -curve(t, [(0, 2), (3, 2), (5.4, 9), (6.8, 30), (7.5, 22), (9, 4), (12, 2)])
            * (dancing ? 1 : 0.3), 0, 0)),
      "mane": JointPose(
        rotation: V3(
          (sin(time * 2.1) * 1.3 - lean * 0.22) * followThrough, -turn * 0.22 * followThrough,
          sin(time * 1.7) * 1.2)),
      "beard": JointPose(
        rotation: V3(sin(time * 2 - 0.4) * 3 * followThrough, 0, sin(time * 1.6) * 2)),
      "tail": JointPose(
        rotation: V3(
          sin(time * 1.6) * 4, sin(time * 1.4 - 0.8) * 12 * followThrough - turn * 0.3, 0)),
    ]
    let matrices = PartRig.matrices(joints, poses: poses)
    var targets: [String: V3] = [:]
    var feet: [String: V3] = [:]
    var contacts: [String: Bool] = [:]
    var reach: Float = 0
    for front in [true, false] {
      for side: Float in [-1, 1] {
        let id = limbID(front, side)
        let rest = restLimb(front, side)
        let parent = matrices[front ? "neck" : "haunch"]!
        let v = parent * SIMD4(rest.0, 1)
        let shoulder = V3(v.x, v.y, v.z)
        let start: Float = front ? (side < 0 ? 3.35 : 4.35) : (side < 0 ? 8.3 : 9.3)
        let local = (t - start) / 1.05
        let lift = local > 0 && local < 1 ? pow(sin(local * .pi), 2) * 0.3 * s : 0
        // The foot's horizontal excursion occurs only while it is airborne.
        let target = rest.2 + V3(side * lift * 0.38, lift, -lift * 0.75)
        let solution = TwoBoneIK.solve(
          root: shoulder, target: target,
          pole: shoulder + V3(side * 0.4, 0, front ? 2 : 2), upper: length(rest.1 - rest.0),
          lower: length(rest.2 - rest.1))
        reach = max(reach, solution.residual)
        poses[id + "-upper"] = JointPose(
          offset: shoulder - rest.0,
          rotation: TwoBoneIK.rotation(from: rest.1 - rest.0, to: solution.joint - shoulder))
        poses[id + "-lower"] = JointPose(
          offset: solution.joint - rest.1,
          rotation: TwoBoneIK.rotation(from: rest.2 - rest.1, to: solution.end - solution.joint))
        poses[id + "-paw"] = JointPose(offset: solution.end - rest.2)
        targets[id] = target
        feet[id] = solution.end
        contacts[id] = lift < 0.001
      }
    }
    let beat = dancing ? (beats.last { $0.time <= t }?.name ?? "Attend") : "Breathe"
    return Frame(poses: poses, targets:targets, feet: feet, contacts: contacts, maximumReachError: reach, beat: beat)
  }
}
