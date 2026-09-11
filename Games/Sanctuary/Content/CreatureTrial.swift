import Foundation
import simd

/// Deterministic encounter fixtures used by the workshop and headless checks.
public enum CreatureScenario: String, Codable, CaseIterable, Sendable {
  case calm, startle, recovery, occluded, forage, obstacle
  public func stimulus(at seconds: Float) -> CreatureStimulus {
    var s = CreatureStimulus()
    switch self {
    case .calm:
      s.player = SIMD2(0, 3)
      s.food = nil
    case .startle:
      s.player = SIMD2(0, 3)
      s.running = seconds >= 1
      s.food = nil
    case .recovery:
      s.player = SIMD2(0, 3)
      s.running = seconds >= 1 && seconds < 2
      s.food = nil
    case .occluded:
      s.player = SIMD2(0, 3)
      s.running = true
      s.visible = false
      s.food = nil
    case .forage: break
    case .obstacle:
      s.food = SIMD2(0, -2)
      s.obstacle = SIMD2(0, -1.1)
      s.obstacleRadius = 0.35
    }
    return s
  }
}
