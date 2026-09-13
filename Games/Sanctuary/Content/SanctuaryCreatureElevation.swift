import Foundation
import simd

/// Game-owned vertical placement shared by wildlife presentation and perception.
/// Heights are absolute world-space metres at the requested horizontal location.
public struct SanctuaryCreatureElevation: Equatable, Sendable {
  public let supportY: Float
  public let rootY: Float
  public let focusY: Float

  public init(supportY: Float, rootY: Float, focusY: Float) {
    self.supportY = supportY
    self.rootY = rootY
    self.focusY = focusY
  }
}

extension WildlifeSpecies {
  /// Flying companions retain their authored clearance from a water surface,
  /// rather than treating the submerged terrain bed as their support.
  var sanctuaryUsesWaterSupport: Bool {
    profile.capabilities.contains(.fly)
  }

  /// Stable clearance already used by the production wildlife presentation.
  var sanctuaryHoverBaseline: Float {
    switch self {
    case .canopyGlider: return 1.35
    case .cloudRay: return 4.8
    default: return 0
    }
  }

  /// Authored eye height above the creature root in metres.
  var sanctuaryFocusOffset: Float {
    switch self {
    case .frostling: return 0.80
    case .sunhare: return 0.82
    case .brookweaver: return 0.48
    case .reedwalker: return 1.48
    case .moonhart: return 1.65
    case .cloudstepper: return 1.28
    case .dunefox: return 0.72
    case .canopyGlider: return 0.53
    case .saltback: return 0.41
    case .tidepooler: return 0.39
    case .cloudRay: return 0.28
    }
  }
}

extension SanctuaryWorld {
  /// Highest saved walkable surface at a horizontal coordinate. Construction
  /// facts come from the same transforms used by player collision and rendering.
  func creatureWalkableSupport(at point: SIMD2<Float>) -> Float? {
    var support: Float?
    for fact in constructionFacts where fact.kind == .walkable
      && fact.contains(.init(x: point.x, y: fact.top, z: point.y))
    {
      support = max(support ?? fact.top, fact.top)
    }
    return support
  }

  /// Resolves one authoritative support, render-root, and perception-focus height.
  /// The optional coordinate supports camera-attached presentation without changing actor state.
  public func elevation(
    for actor: WildlifeActor, at location: SIMD2<Float>? = nil
  ) -> SanctuaryCreatureElevation {
    let point = location ?? actor.position
    let ground = groundHeight(point.x, point.y)
    var support = actor.species.sanctuaryUsesWaterSupport
      ? max(ground, localWaterHeight(point.x, point.y) ?? ground)
      : ground
    if !actor.species.sanctuaryUsesWaterSupport,
      let walkable = creatureWalkableSupport(at: point)
    {
      support = max(support, walkable)
    }
    let root = support + actor.species.sanctuaryHoverBaseline
    return SanctuaryCreatureElevation(
      supportY: support,
      rootY: root,
      focusY: root + actor.species.sanctuaryFocusOffset * actor.morphologyScale)
  }
}
