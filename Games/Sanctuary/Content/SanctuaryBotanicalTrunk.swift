import FieldCore
import Foundation
import simd

/// Metre-valued trunk curves shared by botanical mesh authoring and regional collision.
/// Collision follows only the main trunk; leaves, aerial branches and root fans remain visual.
public struct SanctuaryBotanicalTrunk: Sendable {
  public enum Form: Sendable { case willow, palm, conifer }
  public let form: Form
  public let height: Float
  public let width: Float
  public let trunkRadius: Float

  public init(_ form: Form, height: Float? = nil, width: Float? = nil, radius: Float? = nil) {
    self.form = form
    let defaults: (Float, Float, Float)
    switch form {
    case .willow: defaults = (5.6, 6, 0.27)
    case .palm: defaults = (7, 6.2, 0.19)
    case .conifer: defaults = (8, 5.2, 0.24)
    }
    self.height = height ?? defaults.0
    self.width = width ?? defaults.1
    self.trunkRadius = radius ?? defaults.2
  }

  public func point(at t: Float) -> V3 {
    switch form {
    case .willow:
      return V3(0.19 * sin(t * 2.4) * t, height * 0.68 * t, -0.11 * t * t)
    case .palm:
      return V3(width * (0.13 * t * t - 0.035 * sin(t * .pi)), height * 0.78 * t,
        width * 0.025 * sin(t * 2.3) * t)
    case .conifer:
      return V3(width * 0.023 * sin(t * 2.7) * t, height * t, width * 0.018 * t * t)
    }
  }

  public func radius(at t: Float) -> Float {
    switch form {
    case .willow: return trunkRadius * (1.05 - 0.76 * t)
    case .palm:
      return trunkRadius * (1.28 - 0.50 * t) * (1 + 0.045 * cos(t * 30 * .pi))
    case .conifer: return trunkRadius * (1.12 - 1.07 * t)
    }
  }

  /// Default collision fields are cached once. A short capsule chain conservatively covers
  /// the bending trunk without treating the whole canopy as a solid obstacle.
  public static let willowCollision = SanctuaryBotanicalTrunk(.willow).collisionShape()
  public static let palmCollision = SanctuaryBotanicalTrunk(.palm).collisionShape()
  public static let coniferCollision = SanctuaryBotanicalTrunk(.conifer).collisionShape()

  public func collisionShape() -> Shape {
    // Conifer follows every main-trunk ring of the reviewed mesh source.
    let segments = form == .conifer ? 22 : (form == .palm ? 12 : 8)
    func segment(_ index: Int) -> Shape {
      let t = Float(index) / Float(segments)
      let next = Float(index + 1) / Float(segments)
      // All radius envelopes decrease along the source. Palm bark rings use their upper
      // envelope, and 4 mm covers centerline chord error at these default dimensions.
      let r = form == .palm ? trunkRadius * (1.28 - 0.50 * t) * 1.045 : radius(at: t)
      return .capsule(point(at: t), point(at: next), r + 0.004)
    }
    var result = segment(0)
    for index in 1..<segments { result = result.joined(segment(index)) }
    return result
  }
}
