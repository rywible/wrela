import simd

/// Authored bridge/deck dimensions shared by procedural fields and collision.
/// Local metres, Y up; the saved placement supplies the scale, yaw and base.
/// These dimensions describe the existing shapes, not a structural simulation.
public enum SanctuaryWalkableConstructionProfile {
  public struct Box: Equatable, Sendable {
    public let center: SIMD3<Float>
    public let halfExtents: SIMD3<Float>
    public var top: Float { center.y + halfExtents.y }
  }

  public struct Capsule: Equatable, Sendable {
    public let start: SIMD3<Float>
    public let end: SIMD3<Float>
    public let radius: Float

    /// Conservative local bounds, including the rounded end caps. Consumers
    /// transform this box with the same placement as the rendered capsule.
    public var bounds: Box {
      let padding = SIMD3<Float>(repeating: radius)
      let low = simd_min(start, end) - padding
      let high = simd_max(start, end) + padding
      return Box(center: (low + high) * 0.5, halfExtents: (high - low) * 0.5)
    }
  }

  public struct Profile: Equatable, Sendable {
    public let platform: Box
    /// Horizontal rails followed by posts; the deck has only corner posts.
    /// The order also preserves the original procedural union expressions.
    public let rails: [Capsule]
  }

  public static let bridge = Profile(
    platform: Box(center: SIMD3(0, 0.15, 0), halfExtents: SIMD3(2.68, 0.12, 0.82)),
    rails: [
      Capsule(start: SIMD3(-2.45, 0.80, -0.72), end: SIMD3(2.45, 0.80, -0.72), radius: 0.055),
      Capsule(start: SIMD3(-2.45, 0.80, 0.72), end: SIMD3(2.45, 0.80, 0.72), radius: 0.055),
      Capsule(start: SIMD3(-2.35, 0.10, -0.72), end: SIMD3(-2.35, 0.84, -0.72), radius: 0.06),
      Capsule(start: SIMD3(2.35, 0.10, -0.72), end: SIMD3(2.35, 0.84, -0.72), radius: 0.06),
      Capsule(start: SIMD3(-2.35, 0.10, 0.72), end: SIMD3(-2.35, 0.84, 0.72), radius: 0.06),
      Capsule(start: SIMD3(2.35, 0.10, 0.72), end: SIMD3(2.35, 0.84, 0.72), radius: 0.06),
    ])

  public static let deck = Profile(
    platform: Box(center: SIMD3(0, 0.12, 0), halfExtents: SIMD3(2.08, 0.10, 1.68)),
    rails: [
      Capsule(start: SIMD3(-1.82, 0, -1.43), end: SIMD3(-1.82, 0.55, -1.43), radius: 0.09),
      Capsule(start: SIMD3(1.82, 0, -1.43), end: SIMD3(1.82, 0.55, -1.43), radius: 0.09),
      Capsule(start: SIMD3(-1.82, 0, 1.43), end: SIMD3(-1.82, 0.55, 1.43), radius: 0.09),
      Capsule(start: SIMD3(1.82, 0, 1.43), end: SIMD3(1.82, 0.55, 1.43), radius: 0.09),
    ])
}
