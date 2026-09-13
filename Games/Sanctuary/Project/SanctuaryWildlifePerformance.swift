import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import simd

/// Soundstage adapter for the production 60 Hz wildlife brain. Requests are
/// authored stimuli; acceptance fixtures do not replace relationship decisions.
enum SanctuaryWildlifePerformance {
  private struct State: Codable {
    var version = 1
    var brain: CreatureSimulation
    var stimulus: CreatureStimulus
  }
  struct Frame {
    var brain: CreatureSimulation
    var player: SIMD2<Float>
    var identifier: String
    var time: Float
    var phase: Float?
    var position: SIMD2<Float>
    var yaw: Float
  }

  /// The production decision trace supplies the origin; wall time and studio
  /// playback state never start or restart an animal's acting gesture.
  static func refusalElapsed(_ brain: CreatureSimulation) -> Float? {
    guard !brain.requestAccepted,
      let event = brain.events.last(where: { $0.state == "declining" }),
      brain.tick >= event.tick else { return nil }
    return Float(brain.tick - event.tick) / 60
  }

  private static func state(_ snapshot: BehaviorSnapshot) -> State {
    if let value = try? JSONDecoder().decode(State.self, from: snapshot.data), value.version == 1 {
      return value
    }
    // Production/replay snapshots may carry the original Frostling brain payload.
    let brain = SanctuaryProject.actor(snapshot)
    var stimulus = CreatureStimulus()
    stimulus.player = brain.movementAnchor ?? brain.position + SIMD2(0, -4)
    stimulus.food = nil
    return State(brain: brain, stimulus: stimulus)
  }
  private static func snapshot(_ state: State) -> BehaviorSnapshot {
    var result = SanctuaryProject.snapshot(state.brain)
    let encoder = JSONEncoder(); encoder.outputFormatting = [.sortedKeys]
    result.data = (try? encoder.encode(state)) ?? Data()
    return result
  }

  private static let scenarios = ["greeting-side", "greeting-rear", "accepted-wait",
    "refused-wait", "refused-play", "curiosity", "lost-attention"]

  private static func input(_ scenario: String, _ seconds: Float) -> PreviewInput {
    var value = PreviewInput()
    value.food = nil
    // Rear placement matches the native opening encounter relative to Sunhare's home.
    value.player = scenario == "greeting-side" ? SIMD2(4, 0) : SIMD2(-2, 4.8)
    if scenario == "curiosity" { value.player = SIMD2(3, -2) }
    if scenario == "lost-attention", seconds >= 1.5 {
      value.player = SIMD2(0, -4); value.visible = false
    }
    if scenario != "curiosity" {
      value.values["request"] = scenario == "refused-play" ? 3
        : scenario.hasSuffix("wait") || scenario == "lost-attention" ? 2 : 1
      value.values["accepted"] = scenario.hasPrefix("refused-") ? 0 : 1
    }
    return value
  }

  static func make(identifier: String, flying: Bool,
    poses: @escaping (Frame) -> [String: JointPose],
    root: @escaping (Frame) -> simd_float4x4) -> AnimationDefinition
  {
    let controls: [ScalarControl] = [
      .init("speakerBearing", "Speaker bearing offset · °", 0, -90...90),
      .init("speakerDistance", "Speaker distance multiplier", 1, 0.25...1.5),
      .init("speakerVisible", "Speaker visible · 0/1", 1, 0...1),
      .init("speakerRunning", "Speaker running · 0/1", 0, 0...1),
    ]
    let duration: Float = flying ? 2 * .pi / 0.72 : CreatureMotion().duration
    func frame(_ clip: String, _ seconds: Float, _ snapshot: BehaviorSnapshot) -> Frame {
      let value = state(snapshot)
      let behavior = clip == "behavior"
      let phase: Float? = behavior ? value.brain.phase
        : clip == "hop" ? seconds.truncatingRemainder(dividingBy: duration) / duration : nil
      return Frame(brain: behavior ? value.brain : CreatureSimulation(), player: value.stimulus.player,
        identifier: identifier, time: behavior ? value.brain.time : seconds, phase: phase,
        position: behavior ? value.brain.position - value.brain.home : .zero,
        yaw: behavior ? value.brain.yaw : 0)
    }
    let evaluatePoses: (String, Float, BehaviorSnapshot, MotionParameters) -> [String: JointPose] = {
      clip, seconds, snapshot, _ in
      clip == "bind" ? [:] : poses(frame(clip, seconds, snapshot))
    }
    let evaluateRoot: (String, Float, BehaviorSnapshot, MotionParameters) -> simd_float4x4 = {
      clip, seconds, snapshot, _ in
      clip == "bind" ? matrix_identity_float4x4 : root(frame(clip, seconds, snapshot))
    }
    let initialize: (UInt32) -> BehaviorSnapshot = { seed in
      var stimulus = CreatureStimulus(); stimulus.food = nil
      return snapshot(State(brain: CreatureSimulation(seed: seed), stimulus: stimulus))
    }
    let advance: (BehaviorSnapshot, PreviewInput, MotionParameters) -> BehaviorSnapshot = {
      previous, input, params in
        var value = state(previous)
        var stimulus = CreatureStimulus()
        let angle: Float = (params.values["speakerBearing"] ?? 0) * Float.pi / 180
        let delta: SIMD2<Float> = input.player - value.brain.home
        let rotated: SIMD2<Float> = SIMD2<Float>(
          cos(angle) * delta.x + sin(angle) * delta.y,
          -sin(angle) * delta.x + cos(angle) * delta.y)
        let distanceScale: Float = params.values["speakerDistance"] ?? 1
        stimulus.player = value.brain.home + rotated * distanceScale
        stimulus.visible = input.visible && (params.values["speakerVisible"] ?? 1) >= 0.5
        stimulus.running = input.running || (params.values["speakerRunning"] ?? 0) >= 0.5
        stimulus.food = input.food; stimulus.obstacle = input.obstacle
        stimulus.obstacleRadius = input.obstacleRadius
        if value.brain.tick == 0, let request = input.values["request"] {
          let invitation: AnimalRequest = request == 3 ? .play : request == 2 ? .wait : .greeting
          value.brain.address(invitation,
            accepted: (input.values["accepted"] ?? 1) >= 0.5, player: stimulus.player)
        }
        value.brain.step(stimulus)
        value.stimulus = stimulus
        return snapshot(value)
    }
    let bounds: (String) -> Bounds = { _ in
      flying ? Bounds(V3(-3.5, -0.7, -3.5), V3(3.5, 1, 3.5))
        : Bounds(V3(-1.5, 0, -1.5), V3(1.5, 2.7, 1.5))
    }
    let validate: (MotionParameters) throws -> Void = { params in
        for control in controls {
          let value = params.values[control.key] ?? control.initial
          guard value.isFinite, control.range.contains(value),
            !["speakerVisible", "speakerRunning"].contains(control.key) || value.rounded() == value
          else { throw RuntimeError.message("Invalid wildlife stimulus: \(control.key)") }
        }
    }
    var definition = AnimationDefinition(controls: controls,
      clips: flying ? ["idle", "flight"] : ["idle", "hop"], scenarios: scenarios,
      signalName: "Fear", duration: { (_: MotionParameters) -> Float in duration },
      poses: evaluatePoses, root: evaluateRoot, initialize: initialize, step: advance,
      input: input, bounds: bounds, validate: validate)
    if flying && identifier.hasPrefix("canopyGlider-") {
      definition.motionEnvelope = SanctuaryCanopyGliderDesign.motionEnvelope
    }
    definition.inspect = { (clip: String, seconds: Float, snapshot: BehaviorSnapshot,
      _: MotionParameters) -> [String: Float] in
      let sample: Frame = frame(clip, seconds, snapshot)
      let delta: SIMD2<Float> = sample.player - sample.brain.position
      let desired: Float = atan2(-delta.x, -delta.y)
      let error: Float = atan2(sin(desired - sample.yaw), cos(desired - sample.yaw))
      let grounded: Float = sample.phase.map { (phase: Float) -> Float in
        phase <= 0.22 || phase >= 0.72 ? 1 : 0
      } ?? 1
      return ["brainTick": Float(sample.brain.tick), "gazeDegrees": sample.brain.gaze,
        "bodyYawDegrees": sample.yaw * 180 / .pi, "speakerErrorDegrees": error * 180 / .pi,
        "hopPhase": sample.phase ?? -1, "grounded": grounded,
        "horizontalTravelMetres": length(sample.position), "requestAccepted": sample.brain.requestAccepted ? 1 : 0,
        "refusalElapsedSeconds": refusalElapsed(sample.brain) ?? -1]
    }
    return definition
  }
}
