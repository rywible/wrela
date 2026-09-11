import FieldCore
import Foundation
import simd

/// The first game's rules. No rendering, UI, wall clock, or file access.
public struct Expedition: Codable, Equatable, Sendable {
  public enum Phase: String, Codable, Sendable { case searching, carrying, settled }
  public static let creatureID = "frostling-001"
  public static let home = SIMD2<Float>(0, 18)
  public static let den = SIMD2<Float>(-3, -83)
  public static let signs: [SIMD2<Float>] = [SIMD2(0, -2), SIMD2(-10, -32), SIMD2(-2, -61)]
  public static let signNotes = [
    "Frost on warm grass. Two small tracks head north, along the path.",
    "A tuft of pale fur. The tracks continue toward the old stone gate.",
    "Fresh frost beneath the gate. Something is sheltering just beyond it.",
  ]
  public private(set) var phase: Phase = .searching
  public private(set) var discoveredSigns: Set<Int> = []
  public private(set) var trust: Float = 0
  public private(set) var habitat: Float = 0
  public private(set) var age: Double = 0
  public private(set) var legendaryHint = false
  public var player = SIMD2<Float>(0, 24)
  public var yaw: Float = 0
  public var pitch: Float = -0.04

  public init() {}

  public var creaturePosition: SIMD2<Float> {
    let center = phase == .settled ? Self.home : Self.den
    // Short, bounded excursions keep the signs truthful and the encounter findable.
    let t = Float(age.truncatingRemainder(dividingBy: 4096))
    let wander: Float = phase == .settled ? 1.5 : 0.65
    return center + SIMD2(sin(t * 0.22), sin(t * 0.17 + 0.4)) * wander
  }

  public mutating func advance(seconds: Float, movingQuickly: Bool, visible: Bool = true) {
    guard seconds.isFinite, seconds > 0, seconds <= 1 else { return }
    age += Double(seconds)
    if phase == .searching {
      let near = distance(player, creaturePosition) < 5
      trust = clamp(trust + (near && visible && !movingQuickly ? seconds / 4 : -seconds / 2), 0, 1)
    }
    if phase == .settled { habitat = min(1, habitat + seconds / 18) }
  }

  public var objective: String {
    switch phase {
    case .searching:
      if distance(player, creaturePosition) < 9 {
        return "A Frostling! Approach gently and let it get used to you."
      }
      return discoveredSigns.isEmpty
        ? "Find signs of wildlife along the northern path."
        : "Follow the frost traces toward and beyond the old gate."
    case .carrying:
      return "Bring your Frostling home. Prepare its garden inside the sanctuary stones."
    case .settled:
      return habitat < 1
        ? "Watch your Frostling awaken its frost garden."
        : "Your first resident is home. Visit it to learn what might live farther north."
    }
  }

  public var prompt: String? {
    switch phase {
    case .searching:
      if distance(player, creaturePosition) < 4 {
        return trust >= 0.99 ? "Rescue Frostling" : "Let it know you are a friend"
      }
      if nearestUnreadSign != nil { return "Examine frost traces" }
    case .carrying:
      if distance(player, Self.home) < 7 { return "Prepare frost garden & release" }
    case .settled:
      if distance(player, creaturePosition) < 5 { return "Visit Frostling" }
    }
    return nil
  }

  private var nearestUnreadSign: Int? {
    Self.signs.indices.filter {
      !discoveredSigns.contains($0) && distance(player, Self.signs[$0]) < 4
    }
    .min { distance(player, Self.signs[$0]) < distance(player, Self.signs[$1]) }
  }

  /// The native button, keyboard and agent protocol all call this same action.
  @discardableResult public mutating func interact(visible: Bool = true) -> String {
    switch phase {
    case .searching:
      if distance(player, creaturePosition) < 4 {
        guard visible else { return "Move into a clear view of the Frostling." }
        guard trust >= 0.99 else {
          return "Stay close and move gently. Give it a few seconds to trust you."
        }
        phase = .carrying
        return "The Frostling nestles into your care. Take it back to the sanctuary."
      }
      if let sign = nearestUnreadSign {
        discoveredSigns.insert(sign)
        return Self.signNotes[sign]
      }
    case .carrying:
      if distance(player, Self.home) < 7 {
        phase = .settled
        habitat = 0
        return "A home for your Frostling. Watch the ground bloom with frost."
      }
    case .settled:
      if distance(player, creaturePosition) < 5 {
        guard habitat >= 1 else {
          return "Your Frostling is settling in. Tiny ice flowers are opening around it."
        }
        legendaryHint = true
        return
          "It watches the northern ridge. Old stories speak of a great white creature that carries winter in its antlers…"
      }
    }
    return "Explore the path and look for pale frost traces."
  }

  public func validate() throws {
    guard player.x.isFinite, player.y.isFinite, abs(player.x) <= 130,
      (-135...120).contains(player.y),
      yaw.isFinite, abs(yaw) < 100000, pitch.isFinite, (-1.35...1.35).contains(pitch),
      trust.isFinite, (0...1).contains(trust), habitat.isFinite, (0...1).contains(habitat),
      age.isFinite, (0...1e9).contains(age),
      discoveredSigns.allSatisfy(Self.signs.indices.contains),
      phase == .settled || (habitat == 0 && !legendaryHint)
    else {
      throw ExpeditionError.invalidSave
    }
  }
}

public enum ExpeditionError: LocalizedError {
  case invalidSave, unsupportedVersion, unreadableSave
  public var errorDescription: String? {
    switch self {
    case .invalidSave: return "The expedition save contains invalid state."
    case .unsupportedVersion: return "This expedition was saved by an unsupported version."
    case .unreadableSave:
      return "The expedition save could not be read. The original files have been preserved."
    }
  }
}
