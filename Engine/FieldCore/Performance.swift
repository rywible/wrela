import Foundation
import simd

/// Authoring curves are deterministic at arbitrary time; no hidden integrator state.
public struct MotionKey: Codable, Equatable, Sendable {
  public var time: Float
  public var inTangent: Float?
  public var outTangent: Float?
  public var value: Float
  public init(_ time: Float, _ value: Float) {
    self.time = time
    self.value = value
  }
}
public struct MotionCurve: Codable, Equatable, Sendable {
  public var keys: [MotionKey]
  public init(_ keys: [MotionKey]) { self.keys = keys }
  public func value(at time: Float) -> Float {
    guard let first = keys.first, let last = keys.last else { return 0 }
    if time <= first.time { return first.value }
    if time >= last.time { return last.value }
    for i in 1..<keys.count where time <= keys[i].time {
      let a = keys[i - 1]
      let b = keys[i]
      let t = (time - a.time) / (b.time - a.time)
      if a.outTangent != nil || b.inTangent != nil {
        let dt=b.time-a.time, t2=t*t, t3=t2*t
        return (2*t3-3*t2+1)*a.value+(t3-2*t2+t)*dt*(a.outTangent ?? 0)
          + (-2*t3+3*t2)*b.value+(t3-t2)*dt*(b.inTangent ?? 0)
      }
      let s = t * t * t * (10 + t * (-15 + 6 * t))  // zero velocity AND acceleration at authored holds
      return a.value + (b.value - a.value) * s
    }
    return last.value
  }
  public func validate(duration: Float) throws {
    guard !keys.isEmpty, keys.count <= 256 else { throw PerformanceError.invalid }
    var previous: Float = -1
    for key in keys {
      guard key.time.isFinite, key.value.isFinite, key.time >= 0, key.time <= duration,
        key.time > previous, abs(key.value) <= 360,
        [key.inTangent,key.outTangent].allSatisfy({$0 == nil || ($0!.isFinite && abs($0!)<=720)})
      else { throw PerformanceError.invalid }
      previous = key.time
    }
  }
}
public struct PerformanceBeat: Codable, Equatable, Sendable {
  public var name: String
  public var time: Float
  public var intent: String
  public init(_ name: String, _ time: Float, _ intent: String) {
    self.name = name
    self.time = time
    self.intent = intent
  }
}
public struct PoseTrack: Codable, Equatable, Sendable {
  public var joint: String
  public var channel: String
  public var curve: MotionCurve
  public init(_ joint: String, _ channel: String, _ keys: [MotionKey]) {
    self.joint = joint
    self.channel = channel
    curve = MotionCurve(keys)
  }
}
/// Additive corrections over the generator's performance, saved with the asset.
public struct PerformanceScore: Codable, Equatable, Sendable {
  public var clip: String
  public var duration: Float
  public var beats: [PerformanceBeat]
  public var tracks: [PoseTrack]
  public init(
    clip: String, duration: Float, beats: [PerformanceBeat] = [], tracks: [PoseTrack] = []
  ) {
    self.clip = clip
    self.duration = duration
    self.beats = beats
    self.tracks = tracks
  }
  public func validate(joints: [String]) throws {
    guard duration.isFinite, (0.1...60).contains(duration), tracks.count <= 192,
      beats.count <= 64, !clip.isEmpty
    else { throw PerformanceError.invalid }
    var seen = Set<String>()
    for track in tracks {
      guard joints.contains(track.joint),
        ["x", "y", "z", "pitch", "yaw", "roll"].contains(track.channel),
        seen.insert(track.joint + "/" + track.channel).inserted
      else { throw PerformanceError.invalid }
      try track.curve.validate(duration: duration)
    }
    var last: Float = -1
    for beat in beats {
      guard !beat.name.isEmpty, beat.name.count <= 80, beat.intent.count <= 500,
        beat.time.isFinite, beat.time >= 0, beat.time < duration, beat.time > last
      else { throw PerformanceError.invalid }
      last = beat.time
    }
  }
  public func apply(to poses: [String: JointPose], time: Float) -> [String: JointPose] {
    var poses = poses
    let time = max(0, time).truncatingRemainder(dividingBy: duration)
    for track in tracks {
      var pose = poses[track.joint] ?? JointPose()
      let value = track.curve.value(at: time)
      switch track.channel {
      case "x": pose.offset.x += value
      case "y": pose.offset.y += value
      case "z": pose.offset.z += value
      case "pitch": pose.rotation.x += value
      case "yaw": pose.rotation.y += value
      case "roll": pose.rotation.z += value
      default: break
      }
      poses[track.joint] = pose
    }
    return poses
  }
}
public enum PerformanceError: LocalizedError {
  case invalid
  public var errorDescription: String? {
    "Invalid performance: use ordered finite keys, valid joints/channels and a duration of 0.1…60 seconds."
  }
}

/// Four nonnegative normalized influences; packed identically in Metal.
public struct SkinWeight: Sendable {
  public var joints: SIMD4<UInt32>
  public var weights: SIMD4<Float>
  public init(_ joint: UInt32) {
    joints = SIMD4(repeating: joint)
    weights = SIMD4(1, 0, 0, 0)
  }
  public init(_ a: UInt32, _ b: UInt32, blend: Float) {
    joints = SIMD4(a, b, 0, 0)
    let t = min(1, max(0, blend))
    weights = SIMD4(1 - t, t, 0, 0)
  }
  public func validate(count: Int) -> Bool {
    (0..<4).allSatisfy { joints[$0] < count && weights[$0].isFinite && weights[$0] >= 0 }
      && abs(weights.sum() - 1) < 0.0001
  }
  public func matrix(_ palette: [simd_float4x4]) -> simd_float4x4 {
    palette[Int(joints.x)] * weights.x + palette[Int(joints.y)] * weights.y
      + palette[Int(joints.z)] * weights.z + palette[Int(joints.w)] * weights.w
  }
}
