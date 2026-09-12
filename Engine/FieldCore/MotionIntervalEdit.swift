import Foundation
import simd

/// A joint-local correction over an explicitly bounded interval. Delay samples
/// the existing native phrase; a positive value makes this joint follow later.
public struct MotionIntervalAdjustment: Codable, Equatable, Sendable {
  public var joint: String
  public var offset: V3
  public var rotation: V3
  public var delay: Float
  public init(joint: String, offset: V3 = .zero, rotation: V3 = .zero, delay: Float = 0) {
    self.joint = joint; self.offset = offset; self.rotation = rotation; self.delay = delay
  }
}

/// Source-to-source editing, independent of anatomy, editor state and simulation.
/// Contact anchors and timing are retained byte-for-byte. Protected times retain
/// all original joint transforms. Runtime IK subsequently measures reach; this
/// operation does not mistake preservation of a target for a reachable contact.
public struct MotionIntervalEdit: Codable, Equatable, Sendable {
  public var start: Float
  public var end: Float
  public var fadeIn: Float
  public var fadeOut: Float
  public var adjustments: [MotionIntervalAdjustment]
  public var bounds: [PoseFreedom]
  public var protectedTimes: [Float]
  public init(start: Float, end: Float, fadeIn: Float, fadeOut: Float,
    adjustments: [MotionIntervalAdjustment], bounds: [PoseFreedom] = [], protectedTimes: [Float] = []) {
    self.start = start; self.end = end; self.fadeIn = fadeIn; self.fadeOut = fadeOut
    self.adjustments = adjustments; self.bounds = bounds; self.protectedTimes = protectedTimes
  }

  public func applying(to phrase: MotionPhrase) throws -> MotionPhrase {
    let path = "editInterval.\(phrase.id)"
    try CraftError.require([start, end, fadeIn, fadeOut].allSatisfy(\.isFinite)
      && start >= 0 && end > start && end <= phrase.duration
      && fadeIn > 0 && fadeOut > 0 && fadeIn + fadeOut <= end - start,
      path, "Use an in-range interval with positive fades that fit inside it")
    let ids = Set(phrase.samples[0].poses.keys)
    try CraftError.require((1...48).contains(adjustments.count)
      && Set(adjustments.map(\.joint)).count == adjustments.count,
      path, "Use 1…48 unique joint adjustments")
    for a in adjustments {
      try CraftError.require(ids.contains(a.joint) && CraftMath.finite(a.offset, limit: 2)
        && CraftMath.finite(a.rotation, limit: 90) && a.delay.isFinite && abs(a.delay) <= 2,
        path + "." + a.joint, "Unknown joint or correction beyond 2 m, 90 degrees, or 2 seconds")
    }
    try CraftError.require(protectedTimes.count <= 32 && protectedTimes.allSatisfy {
      $0.isFinite && (0...phrase.duration).contains($0)
    }, path, "Use at most 32 protected times within the phrase")
    try CraftError.require(bounds.count <= 96 && Set(bounds.map { $0.joint + "/" + $0.channel }).count == bounds.count,
      path, "Use at most 96 unique channel bounds")
    for b in bounds {
      try CraftError.require(ids.contains(b.joint) && ["x", "y", "z", "pitch", "yaw", "roll"].contains(b.channel)
        && b.minimum.isFinite && b.maximum.isFinite && b.minimum < b.maximum
        && abs(b.minimum) <= 360 && abs(b.maximum) <= 360,
        path + ".bounds", "Unknown joint/channel or invalid channel bounds")
    }
    func weight(_ time: Float) -> Float {
      guard time > start && time < end else { return 0 }
      var w = min(CraftMath.smooth((time - start) / fadeIn), CraftMath.smooth((end - time) / fadeOut))
      // Protected samples have a smooth local notch rather than an isolated
      // unchanged key that would introduce a visibly abrupt reversal.
      for t in protectedTimes {
        let width = min(fadeIn, fadeOut)
        w *= CraftMath.smooth(abs(time - t) / width)
      }
      return w
    }
    // The local clock must remain monotonic: a long delay through a short fade
    // would play the selected joint backwards. Report this conflict explicitly.
    for a in adjustments where a.delay != 0 {
      var previous: Float = -Float.greatestFiniteMagnitude
      for tick in 0...Int(ceil(phrase.duration * 120)) {
        let t = min(phrase.duration, Float(tick) / 120)
        let clock = t - a.delay * weight(t)
        try CraftError.require(clock >= previous - 0.000001, path + "." + a.joint + ".delay",
          "Delay reverses the local clock near \(t) s; enlarge the fade or reduce the delay")
        previous = clock
      }
    }
    // Preserve original control times and add bounded review-resolution controls
    // only within the edit. Existing interpolation remains native and unchanged.
    var times = Set(phrase.samples.map(\.time) + protectedTimes + [start, start + fadeIn, end - fadeOut, end])
    let intervals = Int(ceil((end - start) * 10))
    for i in 0...intervals { times.insert(start + (end - start) * Float(i) / Float(intervals)) }
    let ordered = times.sorted().reduce(into: [Float]()) { values, t in
      if values.last.map({ abs($0 - t) > 0.00001 }) ?? true { values.append(t) }
    }
    try CraftError.require(ordered.count <= 241, path + ".samples",
      "Edit needs \(ordered.count) pose samples (limit 241); simplify the phrase or narrow the interval")
    var result = phrase
    result.samples = ordered.map { t in
      let w = weight(t)
      var poses = phrase.pose(at: t)
      if w > 0 {
        for a in adjustments {
          var p = a.delay == 0 ? poses[a.joint]! : phrase.pose(at: max(0, min(phrase.duration, t - a.delay * w)))[a.joint]!
          p.offset += a.offset * w
          p.rotation = PoseBlending.euler(PoseBlending.quaternion(p.rotation) * PoseBlending.quaternion(a.rotation * w))
          poses[a.joint] = p
        }
      }
      return PoseSample(time: t, poses: poses)
    }
    func channel(_ p: JointPose, _ name: String) -> Float {
      switch name {
      case "x": return p.offset.x
      case "y": return p.offset.y
      case "z": return p.offset.z
      case "pitch": return p.rotation.x
      case "yaw": return p.rotation.y
      default: return p.rotation.z
      }
    }
    // Inspect the interpolated candidate, not just keys: splines can overshoot.
    for tick in 0...Int(ceil(phrase.duration * 60)) {
      let t = min(phrase.duration, Float(tick) / 60), poses = result.pose(at: t)
      for b in bounds {
        let v = channel(poses[b.joint]!, b.channel), residual = max(b.minimum - v, v - b.maximum)
        try CraftError.require(residual <= 0.00001, path + "." + b.joint + "." + b.channel,
          "Channel exceeds [\(b.minimum), \(b.maximum)] by \(residual) at \(t) s; revise the correction or its bounds")
      }
    }
    try result.validate(joints: ids, chains: Set(phrase.contacts.map(\.chain)))
    return result
  }
}
