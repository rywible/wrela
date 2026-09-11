import FieldCore
import Foundation
import simd

/// Tunable, asset-owned motion. Fixed feet at rest, one coordinated support/flight
/// cycle when travelling. This is articulated procedural animation, not skinning.
public struct CreatureMotion: Codable, Equatable, Sendable {
  public var blinkRate: Float = 1
  public var attention: Float = 1
  public var duration: Float = 0.85
  public var stride: Float = 0.72
  public var height: Float = 0.17
  public var crouch: Float = 0.075
  public var earFollow: Float = 0.75
  public var breath: Float = 0.009
  public init() {}
  public static let ranges: [String: ClosedRange<Float>] = [
    "duration": 0.5...1.5, "stride": 0.2...1.2, "height": 0.04...0.35,
    "blinkRate": 0...2, "attention": 0...1.5, "crouch": 0...0.12, "earFollow": 0...1.5,
    "breath": 0...0.025,
  ]
  public var values: [String: Float] {
    [
      "duration": duration, "stride": stride, "height": height, "crouch": crouch,
      "earFollow": earFollow, "breath": breath, "blinkRate": blinkRate, "attention": attention,
    ]
  }
  public mutating func set(_ key: String, _ value: Float) throws {
    guard let range = Self.ranges[key], value.isFinite, range.contains(value) else {
      throw RigError.invalid
    }
    switch key {
    case "duration": duration = value
    case "stride": stride = value
    case "height": height = value
    case "crouch": crouch = value
    case "blinkRate": blinkRate = value
    case "attention": attention = value
    case "earFollow": earFollow = value
    default: breath = value
    }
  }
  public func validate() throws {
    for (key, value) in values {
      guard value.isFinite, Self.ranges[key]!.contains(value) else { throw RigError.invalid }
    }
  }
  public static func smooth(_ x: Float) -> Float {
    let t = min(1, max(0, x))
    return t * t * (3 - 2 * t)
  }
  /// Translation occurs only in flight. Stance periods cannot skate by construction.
  public static func travel(_ phase: Float) -> Float { smooth((phase - 0.22) / 0.5) }
  public static func flight(_ phase: Float) -> Float {
    let t = min(1, max(0, (phase - 0.22) / 0.5))
    return sin(t * .pi)
  }
  static func noise(_ cell: Int) -> Float {
    var x = UInt32(truncatingIfNeeded: cell) &* 747_796_405 &+ 2_891_336_453
    x = ((x >> ((x >> 28) &+ 4)) ^ x) &* 277_803_737
    return Float((x >> 22) ^ x) / Float(UInt32.max)
  }
  public func blink(at time: Float) -> Float {
    guard blinkRate > 0 else { return 0 }
    let clock = time * blinkRate
    let cell = Int(floor(clock / 5.5))
    let local = clock - Float(cell) * 5.5
    let start = 1.5 + Self.noise(cell + 73) * 2.6
    func lid(_ t: Float) -> Float {
      Self.smooth(t / 0.055) * (1 - Self.smooth((t - 0.085) / 0.12))
    }
    return max(
      lid((local - start) / blinkRate),
      Self.noise(cell + 91) > 0.7 ? lid((local - start - 0.32) / blinkRate) : 0)
  }
  public func poses(time: Float, phase: Float?, alert: Float = 0, gaze: Float = 0) -> [String:
    JointPose]
  {
    let p = phase ?? 0
    let air = phase == nil ? 0 : Self.flight(p)
    let prep = phase == nil ? 0 : sin(min(1, p / 0.22) * .pi)
    let land = phase == nil ? 0 : sin(min(1, max(0, (p - 0.72) / 0.28)) * .pi)
    let bodyY =
      -crouch * (prep + land * 0.65) + sin(time * 2.1 + sin(time * 0.43) * 0.25) * breath
      * (1 - air)
    let pitch = -prep * 9 + air * cos((p - 0.22) / 0.5 * .pi) * 12 + land * 5
    let ears = (prep * 15 - air * 20 + land * 12) * earFollow
    let cell = Int(floor(time / 3.7))
    let transition = Self.smooth((time - Float(cell) * 3.7) / 0.8)
    func attentionValue(_ salt: Int) -> Float {
      let a = Self.noise(cell - 1 + salt) * 2 - 1
      let b = Self.noise(cell + salt) * 2 - 1
      return (a + (b - a) * transition) * attention * (1 - air) * (1 - alert * 0.65)
    }
    func twitch(_ offset: Float) -> Float {
      let t = (time + offset).truncatingRemainder(dividingBy: 7.3)
      return t < 0.6 ? sin(t * 17) * exp(-t * 7) * 6 * attention : 0
    }
    var result: [String: JointPose] = [
      "eyes": JointPose(scale: SIMD3(1, 1 - blink(at: time) * 0.96, 1)),
      "nose": JointPose(offset: SIMD3(0, sin(time * 17) * 0.0015 * (1 - air), 0)),
      "body": JointPose(offset: SIMD3(0, bodyY, 0), rotation: SIMD3(pitch, 0, 0)),
      "head": JointPose(
        rotation: SIMD3(
          -pitch * 0.6 - alert * 5 + attentionValue(11) * 5, gaze + attentionValue(33) * 11,
          attentionValue(51) * 3)),
      "ear-left": JointPose(
        rotation: SIMD3(
          ears + twitch(0.9), attentionValue(21) * 5, -alert * 9 + attentionValue(15) * 3)),
      "ear-right": JointPose(
        rotation: SIMD3(
          ears * 0.85 + twitch(3.1), attentionValue(28) * 4, alert * 7 + attentionValue(29) * 4)),
      "tail": JointPose(rotation: SIMD3(-pitch * 0.6, 0, 0)),
    ]
    // Paws are root children: body breathing/crouch never drags the support feet.
    for id in ["front-left", "front-right", "hind-left", "hind-right"] {
      let front = id.hasPrefix("front")
      result[id] = JointPose(
        offset: SIMD3(0, air * (front ? 0.055 : 0.035), air * (front ? 0.06 : -0.07)),
        rotation: SIMD3(air * (front ? -25 : 18), 0, 0))
    }
    return result
  }
}

// Old assets/studies inherit newly introduced expression channels.
extension CreatureMotion {
  enum CodingKeys: String, CodingKey {
    case duration, stride, height, crouch, earFollow, breath, blinkRate, attention
  }
  public init(from decoder: Decoder) throws {
    self.init()
    let c = try decoder.container(keyedBy: CodingKeys.self)
    for (key, _) in Self.ranges {
      if let codingKey = CodingKeys(rawValue: key),
        let value = try c.decodeIfPresent(Float.self, forKey: codingKey)
      {
        try set(key, value)
      }
    }
  }
}
