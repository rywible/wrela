import Foundation
import simd

public struct CreatureStimulus: Codable, Equatable, Sendable {
  public var player: SIMD2<Float> = SIMD2(0, 20)
  public var visible = true
  public var running = false
  public var food: SIMD2<Float>? = SIMD2(1.5, -1)
  public var obstacle: SIMD2<Float>?
  public var obstacleRadius: Float = 0.6
  /// A game-owned ecological destination. It is a stimulus rather than saved
  /// brain state; the population owns why the animal is moving there.
  public var habitatTarget: SIMD2<Float>?
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
  public private(set) var activeRequest: AnimalRequest?
  public private(set) var requestAccepted = true
  public private(set) var movementAnchor: SIMD2<Float>?
  private var requestTicks = 0
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
  /// Records a validated request in the ordinary deterministic brain. Visible
  /// responses are gaze, posture/state and subsequent locomotion.
  public mutating func address(
    _ request: AnimalRequest, accepted: Bool, player: SIMD2<Float>
  ) {
    activeRequest = request
    requestAccepted = accepted
    movementAnchor = boundedSocialAnchor(player)
    // Following persists until the player gives another request. It is an
    // explicit relationship state, not a short animation timer.
    requestTicks = accepted && request == .follow ? 0 : (request == .wait ? 3_600 : 180)
    cooldown = 0
    let delta = player - position
    if length_squared(delta) > 0.0001 {
      let desired = atan2(-delta.x, -delta.y) - yaw
      let toward = max(-30, min(30, atan2(sin(desired), cos(desired)) * 180 / .pi))
      gaze = accepted ? toward : (toward >= 0 ? -25 : 25)
    }
    if accepted {
      decide(request.rawValue, "Responding through movement and attention")
    } else {
      decide("declining", "Keeping distance from an unwelcome request")
    }
  }

  private func boundedSocialAnchor(_ point: SIMD2<Float>) -> SIMD2<Float> {
    let delta = point - home
    return length(delta) > 10 ? home + normalize(delta) * 10 : point
  }

  /// A social pause still acknowledges the speaker with the body. Turn only at a
  /// grounded decision boundary, using the ordinary hop's stance/flight easing.
  /// Two turns can cover a speaker behind us without a single near-180-degree spin.
  /// The saved request anchor supplies last-seen attention when visibility is lost.
  private mutating func turnTowardSpeaker(_ input: CreatureStimulus, motion: CreatureMotion) {
    if input.visible { movementAnchor = boundedSocialAnchor(input.player) }
    guard let speaker = movementAnchor else { return }
    let delta = speaker - position
    guard length_squared(delta) > 0.0001 else { return }
    let desired = atan2(-delta.x, -delta.y)
    let difference = atan2(sin(desired - yaw), cos(desired - yaw))
    guard abs(difference) > 0.25 else { return }
    start = position
    target = position
    hopTick = 0
    hopFrames = Int((motion.duration * 60).rounded())
    startYaw = yaw
    targetYaw = yaw + max(-Float.pi / 2, min(Float.pi / 2, difference))
    hopping = true
  }

  /// Moves only the invisible home-range anchor after a completed hop. The
  /// animal itself still covers every metre through ordinary traversable hops.
  private mutating func advanceFollowingHome(toward point: SIMD2<Float>, stride: Float) {
    let delta = point - home
    guard length(delta) > 7 else { return }
    let shift = min(stride, length(delta) - 6)
    let candidate = home + normalize(delta) * shift
    guard [position, start, target].allSatisfy({ distance($0, candidate) <= 10.1 }) else { return }
    home = candidate
  }

  public mutating func relocateHome(_ newHome: SIMD2<Float>) {
    home = newHome
    position = newHome
    start = newHome
    target = newHome
    hopping = false
    hopTick = 0
    activeRequest = nil
    requestAccepted = true
    movementAnchor = nil
    requestTicks = 0
    cooldown = 45
  }

  /// Commits a nearby habitat destination without snapping the animal to it.
  /// Callers first move the ordinary brain within settling range.
  public mutating func settleHome(_ newHome: SIMD2<Float>) {
    guard newHome.x.isFinite, newHome.y.isFinite, distance(position, newHome) <= 0.5 else { return }
    home = newHome
    start = position
    target = position
    hopping = false
    hopTick = 0
    movementAnchor = nil
    cooldown = 45
  }
  public mutating func step(
    _ input: CreatureStimulus, motion: CreatureMotion = CreatureMotion(),
    canTraverse: ((SIMD2<Float>, SIMD2<Float>) -> Bool)? = nil
  ) {
    tick += 1
    if requestTicks > 0 {
      requestTicks -= 1
      if requestTicks == 0 {
        activeRequest = nil
        requestAccepted = true
      }
    }
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
    } else if let request = activeRequest {
      if !requestAccepted {
        decide("declining", "Keeping distance from an unwelcome request")
        return
      }
      switch request {
      case .greeting:
        decide("greeting", "Returning the visitor's attention")
        turnTowardSpeaker(input, motion: motion)
        return
      case .wait:
        decide("waiting", "Waiting where asked")
        turnTowardSpeaker(input, motion: motion)
        return
      case .come:
        if input.visible { movementAnchor = boundedSocialAnchor(input.player) }
        if d <= 1.5 {
          decide("waiting", "Reached the familiar visitor")
          activeRequest = nil
          requestTicks = 0
          return
        }
        decide("coming", "Approaching a familiar visitor")
        destination = movementAnchor
      case .follow:
        if input.visible {
          advanceFollowingHome(toward: input.player, stride: motion.stride)
          movementAnchor = boundedSocialAnchor(input.player)
        }
        if d <= 1.8 {
          decide("following", "Keeping near a trusted visitor")
          return
        }
        decide("following", "Moving with a trusted visitor")
        destination = movementAnchor
      case .play:
        if input.visible { movementAnchor = boundedSocialAnchor(input.player) }
        let center = movementAnchor ?? input.player
        let delta = center - position
        let direction = length(delta) > 0.001 ? normalize(delta) : SIMD2<Float>(0, -1)
        let side = SIMD2<Float>(-direction.y, direction.x)
        destination = boundedSocialAnchor(center + side * 1.2)
        decide("playing", "Bounding around the visitor")
      }
    } else if let habitat = input.habitatTarget {
      if distance(position, habitat) <= 0.35 {
        decide("habitat-work", "Tending a chosen habitat place")
        return
      }
      decide("relocating", "Moving toward suitable nearby habitat")
      destination = habitat
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
    let social = activeRequest == .follow || activeRequest == .come || activeRequest == .play
    let leashLimit: Float = social || input.habitatTarget != nil ? 10 : 2.8
    let leash = goal - home
    if length(leash) > leashLimit {
      let boundary = home + normalize(leash) * leashLimit
      let correction = boundary - position
      goal =
        length(correction) > motion.stride
        ? position + normalize(correction) * motion.stride : boundary
    }
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
        if !blocked(left), distance(left, home) <= leashLimit {
          goal = left
        } else if !blocked(right), distance(right, home) <= leashLimit {
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
        p.x.isFinite && p.y.isFinite && abs(p.x) < 20000 && abs(p.y) < 20000
      }),
      yaw.isFinite, startYaw.isFinite, targetYaw.isFinite, gaze.isFinite, abs(gaze) <= 31,
      (0...1).contains(fear),
      (0...1_000_000_000).contains(tick),
      (30...90).contains(hopFrames), (0...hopFrames).contains(hopTick),
      (-1...120).contains(cooldown), distance(position, home) <= 10.1,
      distance(start, home) <= 10.1,
      distance(target, home) <= 10.1,
      movementAnchor.map({ $0.x.isFinite && $0.y.isFinite && distance($0, home) <= 10.1 }) ?? true,
      events.count <= 32,
      events.allSatisfy({ $0.tick >= 0 && $0.tick <= tick && $0.reason.count < 100 }),
      reason.count < 100, state.count < 30, (0...3_600).contains(requestTicks)
    else { throw ExpeditionError.invalidSave }
  }
}

extension CreatureSimulation {
  enum CodingKeys: String, CodingKey {
    case position, yaw, tick, fear, state, reason, events, hopTick, hopFrames, hopping, start,
      target, home, seed, cooldown, startYaw, targetYaw, gaze, activeRequest, requestAccepted,
      movementAnchor, requestTicks
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
    activeRequest = try c.decodeIfPresent(AnimalRequest.self, forKey: .activeRequest)
    requestAccepted = try c.decodeIfPresent(Bool.self, forKey: .requestAccepted) ?? true
    movementAnchor = try c.decodeIfPresent(SIMD2<Float>.self, forKey: .movementAnchor)
    requestTicks = try c.decodeIfPresent(Int.self, forKey: .requestTicks) ?? 0
    try validate()
  }
}
