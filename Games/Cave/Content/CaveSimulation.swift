import FieldCore
import Foundation
import simd

public struct CaveSimulation: Codable, Equatable {
  public var version = 1
  public var seed: UInt32 = 19
  public var time: Float = 0
  public var phase = "searching"
  public var encounter = "dormant"
  public var reason = "The survey beacon stopped transmitting. Find it deeper inside."
  public var monster = V3(-2.4, 0, -18)
  public var seen: Float = 0
  public var retreat: Float = 0
  public var scares = 0
  public var observedFirst = false
  public var returnedScare = false
  public var events: [String] = []
  public init(seed: UInt32 = 19) {
    self.seed = seed
    monster.x = -2.4 + (seed % 2 == 0 ? -0.3 : 0.3)
  }
  public var monsterVisible: Bool {
    encounter == "watching" || encounter == "rush" || (encounter == "withdrawing" && retreat < 0.7)
  }
  public mutating func step(seconds: Float = 1 / 60, player: V3, forward: V3, illuminated: Bool) {
    time += seconds
    if phase == "escaped" {
      encounter = "dormant"
      return
    }
    if encounter == "withdrawing" {
      retreat += seconds
      monster.z -= seconds * 4
      if retreat > 1 { encounter = "dormant" }
    } else if encounter == "rush" {
      retreat += seconds
      monster = player + forward * max(0.65, 3.4 - retreat * 8) - V3(0, 1.4, 0)
      if retreat > 0.38 {
        encounter = "withdrawing"
        retreat = 0
        reason = "Keep moving. The entrance is behind the white trail markers."
      }
    }
    if !observedFirst && player.z < -10 && phase == "searching" {
      if encounter == "dormant" {
        encounter = "watching"
        reason = "A scrape beyond the next bend."
        record(reason)
      }
      seen = illuminated ? seen + seconds : max(0, seen - seconds * 0.4)
      if seen > 0.22 {
        observedFirst = true
        encounter = "withdrawing"
        retreat = 0
        scares += 1
        reason = "Something moved out of your light."
        record(reason)
      }
    }
    if phase == "returning" && player.z > -29 && !returnedScare && illuminated {
      returnedScare = true
      encounter = "rush"
      retreat = 0
      scares += 1
      reason = "RUN."
      record("Return encounter: flashlight acquired the figure")
    }
    if phase == "returning" && player.z > 3 {
      phase = "escaped"
      encounter = "dormant"
      reason = "You made it out. The beacon recorded a second set of footsteps."
      record(reason)
    }
  }
  public mutating func recoverBeacon(player: V3) -> Bool {
    guard phase == "searching", distance(player, CaveLayout.relic) < 3 else { return false }
    phase = "returning"
    encounter = "watching"
    monster = V3(3, 0, -27)
    reason = "Beacon recovered. Return to the entrance."
    record(reason)
    return true
  }
  mutating func record(_ text: String) {
    events.append(text)
    if events.count > 24 { events.removeFirst() }
  }
  public func validate() throws {
    guard version == 1, ["searching", "returning", "escaped"].contains(phase),
      ["dormant", "watching", "withdrawing", "rush"].contains(encounter), time.isFinite,
      (0...1_000_000).contains(time),
      [monster.x, monster.y, monster.z, seen, retreat].allSatisfy(\.isFinite),
      (0...2).contains(scares), events.count <= 24
    else { throw CaveError.invalidSave }
  }
}
public enum CaveError: Error { case invalidSave }
