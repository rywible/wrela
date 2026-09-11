import Foundation
import simd

public struct CreatureStimulus: Codable, Equatable, Sendable {
  public var player: SIMD2<Float> = SIMD2(0, 20)
  public var visible = true
  public var running = false
  public var food: SIMD2<Float>? = SIMD2(1.5, -1)
  public var obstacle: SIMD2<Float>?
  public var obstacleRadius: Float = 0.6
  public init() {}
}
public struct CreatureEvent: Codable, Equatable, Sendable {
  public var tick: Int
  public var state: String
  public var reason: String
}
/// Shared deterministic 60 Hz brain and locomotion, independent of rendering.
/// Decisions change at grounded boundaries, so a startle cannot teleport a foot.
public struct CreatureSimulation: Codable, Equatable, Sendable {
  public private(set) var position: SIMD2<Float>
  public private(set) var yaw: Float = 0
  public private(set) var tick: Int = 0
  public private(set) var gaze: Float = 0
  public private(set) var fear: Float = 0
  public private(set) var state = "resting"
  public private(set) var reason = "Listening at home"
  public private(set) var events: [CreatureEvent] = []
  public private(set) var hopTick: Int = 0
  public private(set) var hopFrames: Int = 51
  public private(set) var hopping = false
  public private(set) var start: SIMD2<Float>
  public private(set) var target: SIMD2<Float>
  public private(set) var home: SIMD2<Float>
  public private(set) var seed: UInt32
  private var cooldown = 90
  private var startYaw: Float = 0
  private var targetYaw: Float = 0
  public init(home: SIMD2<Float> = .zero, seed: UInt32 = 17) {
    self.home = home
    self.position = home
    self.start = home
    self.target = home
    self.seed = seed
  }
  public var phase: Float? { hopping ? Float(hopTick) / Float(hopFrames) : nil }
  public var time: Float { Float(tick % 216000) / 60 }
  private mutating func random() -> Float {
    seed = seed &* 1_664_525 &+ 1_013_904_223
    return Float(seed >> 8) / 16_777_216
  }
  private mutating func decide(_ next: String, _ why: String) {
    if next != state || why != reason {
      events.append(CreatureEvent(tick: tick, state: next, reason: why))
      if events.count > 32 { events.removeFirst(events.count - 32) }
    }
    state = next
    reason = why
  }
  public mutating func step(
    _ input: CreatureStimulus, motion: CreatureMotion = CreatureMotion(),
    canTraverse: ((SIMD2<Float>, SIMD2<Float>) -> Bool)? = nil
  ) {
    tick += 1
    let d = distance(position, input.player)
    let desiredGaze =
      input.visible && d < 8
      ? atan2(-(input.player.x - position.x), -(input.player.y - position.y)) - yaw : 0
    let boundedGaze = max(-30, min(30, atan2(sin(desiredGaze), cos(desiredGaze)) * 180 / .pi))
    gaze += (boundedGaze - gaze) * 0.07
    let threat = input.visible && input.running && d < 7
    fear = min(1, max(0, fear + (threat ? 0.07 : -0.0035)))
    if hopping {
      hopTick += 1
      position = start + (target - start) * CreatureMotion.travel(Float(hopTick) / Float(hopFrames))
      yaw =
        startYaw + atan2(sin(targetYaw - startYaw), cos(targetYaw - startYaw))
        * CreatureMotion.travel(Float(hopTick) / Float(hopFrames))
      if hopTick >= hopFrames {
        hopping = false
        position = target
        cooldown = state == "fleeing" ? 4 : 45
      }
      return
    }
    var destination: SIMD2<Float>?
    if fear > 0.35 {
      if state != "fleeing" { cooldown = min(cooldown, 4) }
      decide("fleeing", threat ? "Visible fast approach" : "Recovering from startle")
      let delta = position - input.player
      let away = length(delta) > 0.001 ? normalize(delta) : SIMD2<Float>(0, -1)
      destination = position + away * motion.stride
    } else if input.visible && d < 5 {
      decide("watching", "Nearby calm visitor")
      let desired = atan2(-(input.player.x - position.x), -(input.player.y - position.y))
      let difference = atan2(sin(desired - yaw), cos(desired - yaw))
      if abs(difference) > 0.25 {
        start = position
        target = position
        hopTick = 0
        hopFrames = Int((motion.duration * 60).rounded())
        startYaw = yaw
        targetYaw = desired
        hopping = true
        reason = "Turning toward calm visitor"
      }
      return
    } else if let food = input.food, distance(position, food) > 0.3 {
      decide("foraging", "Food within home range")
      destination = food
    } else if input.food != nil {
      decide("feeding", "Reached food; staying to forage")
      return
    } else {
      decide("resting", "Safe; pausing between excursions")
      if cooldown <= 0 {
        let a = random() * 2 * Float.pi
        destination = home + SIMD2(cos(a), sin(a)) * (0.4 + random() * 1.0)
        decide("exploring", "Seeded nearby excursion")
      }
    }
    cooldown -= 1
    guard cooldown <= 0, var goal = destination else { return }
    let delta = goal - position
    if length(delta) > motion.stride { goal = position + normalize(delta) * motion.stride }
    let leash = goal - home
    if length(leash) > 2.8 { goal = home + normalize(leash) * 2.8 }
    if input.obstacle != nil || canTraverse != nil {
      func blocked(_ end: SIMD2<Float>) -> Bool {
        if let canTraverse, !canTraverse(position, end) { return true }
        guard let obstacle = input.obstacle else { return false }
        let v = end - position
        let t = max(0, min(1, dot(obstacle - position, v) / max(0.0001, dot(v, v))))
        return distance(position + v * t, obstacle) < input.obstacleRadius + 0.38
      }
      if blocked(goal) {
        let side = SIMD2(-(goal - position).y, (goal - position).x)
        let left = position + side
        let right = position - side
        if !blocked(left), distance(left, home) <= 2.8 {
          goal = left
        } else if !blocked(right), distance(right, home) <= 2.8 {
          goal = right
        } else {
          decide("blocked", "No clear hop landing")
          cooldown = 30
          return
        }
        decide("avoiding", "Obstacle intersects the hop path")
      }
    }
    guard distance(goal, position) > 0.03 else {
      cooldown = 30
      return
    }
    start = position
    target = goal
    hopTick = 0
    hopFrames = Int((motion.duration * 60).rounded())
    hopping = true
    startYaw = yaw
    targetYaw = atan2(-(goal.x - position.x), -(goal.y - position.y))
  }
  public func validate() throws {
    guard
      [position, start, target, home].allSatisfy({ p in
        p.x.isFinite && p.y.isFinite && abs(p.x) < 10000 && abs(p.y) < 10000
      }),
      yaw.isFinite, startYaw.isFinite, targetYaw.isFinite, gaze.isFinite, abs(gaze) <= 31,
      (0...1).contains(fear),
      (0...1_000_000_000).contains(tick),
      (30...90).contains(hopFrames), (0...hopFrames).contains(hopTick),
      (-1...120).contains(cooldown), distance(position, home) <= 3, distance(target, home) <= 3,
      events.count <= 32,
      events.allSatisfy({ $0.tick >= 0 && $0.tick <= tick && $0.reason.count < 100 }),
      reason.count < 100, state.count < 30
    else { throw ExpeditionError.invalidSave }
  }
}

extension CreatureSimulation {
  enum CodingKeys: String, CodingKey {
    case position, yaw, tick, fear, state, reason, events, hopTick, hopFrames, hopping, start,
      target, home, seed, cooldown, startYaw, targetYaw, gaze
  }
  public init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    self.init(
      home: try c.decode(SIMD2<Float>.self, forKey: .home),
      seed: try c.decode(UInt32.self, forKey: .seed))
    position = try c.decode(SIMD2<Float>.self, forKey: .position)
    start = try c.decode(SIMD2<Float>.self, forKey: .start)
    target = try c.decode(SIMD2<Float>.self, forKey: .target)
    yaw = try c.decode(Float.self, forKey: .yaw)
    tick = try c.decode(Int.self, forKey: .tick)
    fear = try c.decode(Float.self, forKey: .fear)
    state = try c.decode(String.self, forKey: .state)
    reason = try c.decode(String.self, forKey: .reason)
    events = try c.decode([CreatureEvent].self, forKey: .events)
    hopTick = try c.decode(Int.self, forKey: .hopTick)
    hopFrames = try c.decode(Int.self, forKey: .hopFrames)
    hopping = try c.decode(Bool.self, forKey: .hopping)
    cooldown = try c.decode(Int.self, forKey: .cooldown)
    startYaw = try c.decodeIfPresent(Float.self, forKey: .startYaw) ?? yaw
    targetYaw = try c.decodeIfPresent(Float.self, forKey: .targetYaw) ?? yaw
    gaze = try c.decodeIfPresent(Float.self, forKey: .gaze) ?? 0
    try validate()
  }
}
