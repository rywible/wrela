import FieldCore
import FieldEngine
import Foundation
import simd

/// Entire authoring clock and brain snapshot travel with a study and undo entry.
struct CreatureWorkshop: Codable {
  var rehearsal = RehearsalSurface()
  var contactView = false
  var contactSolving = true
  var rigView = false
  var recording: String?
  var mode = "bind"
  var scenario = ""
  var generator = ""
  var authoredAnimation:AuthoredAnimation?
  var definition: AnimationDefinition? { authoredAnimation?.resolve(ProjectContext.generator(generator)?.animation) ?? ProjectContext.generator(generator)?.animation }
  var seconds: Float = 0
  var rate: Float = 1
  var remainder: Float = 0
  var seed: UInt32 = 17
  var actor = BehaviorSnapshot()
  var follow = true
  var stimulusOverride: PreviewInput?
  var selected: String?
  var isolated = false
  var focusPart = false
  var modes: [String] { ["bind"] + (definition?.clips ?? []) + ["behavior"] }
  var input: PreviewInput {
    stimulusOverride ?? definition?.input(scenario, seconds) ?? PreviewInput()
  }
  mutating func step(_ dt: Float, motion: MotionParameters, definition: AnimationDefinition? = nil)
  {
    guard mode != "bind" else { return }
    remainder += dt * rate
    while remainder >= 1 / 60 {
      remainder -= 1 / 60
      if seconds >= 60 { reset() }
      if mode == "behavior" {
        actor = (definition ?? self.definition)?.step(actor, input, motion) ?? actor
      }
      seconds += 1 / 60
    }
  }
  mutating func reset() {
    recording = nil
    seconds = 0
    remainder = 0
    actor = definition?.initialize(seed) ?? BehaviorSnapshot()
  }
  mutating func seek(_ time: Float, motion: MotionParameters) throws {
    guard time.isFinite, (0...60).contains(time) else {
      throw RuntimeError.message("Timeline is 0...60 seconds")
    }
    reset()
    for _ in 0..<Int((time * 60).rounded()) {
      if mode == "behavior" { actor = definition?.step(actor, input, motion) ?? actor }
      seconds += 1 / 60
    }
  }
  func validate(source: AssetSource) throws {
    try rehearsal.validate()
    guard modes.contains(mode), seconds.isFinite, (0...60.1).contains(seconds),
      rate.isFinite, (0.1...2).contains(rate), remainder.isFinite, (0...0.017).contains(remainder),
      selected == nil || source.resolvedParts.contains(where: { $0.key == selected }),
      mode == "bind" || source.animation != nil
    else {
      throw RuntimeError.message("Invalid creature study; select an asset with a motion recipe")
    }
    if let i = stimulusOverride {
      guard
        [i.player, i.food ?? .zero, i.obstacle ?? .zero].allSatisfy({
          abs($0.x) <= 20 && abs($0.y) <= 20
        }),
        i.obstacleRadius.isFinite, (0.1...2).contains(i.obstacleRadius)
      else { throw RuntimeError.message("Invalid stimulus") }
    }
    guard actor.position.x.isFinite, actor.position.y.isFinite, actor.fear.isFinite else {
      throw RuntimeError.message("Invalid behavior state")
    }
  }
  func phase(_ motion: MotionParameters) -> Float? {
    if mode == "behavior" { return actor.phase }
    guard mode != "bind", let duration = definition?.duration(motion), duration > 0 else {
      return nil
    }
    return seconds.truncatingRemainder(dividingBy: duration) / duration
  }
  func matrices(source: AssetSource) -> [String: simd_float4x4] {
    evaluatedPose(source:source).matrices
  }
  func evaluatedPose(source: AssetSource) -> ContactResult {
    let frame=root(source.motion ?? MotionParameters())
    func height(_ x:Float,_ z:Float)->Float {ContactRig.localHeight(frame:frame,x:x,z:z,height:rehearsal.height)}
    return source.evaluatedPose(clip:mode,time:seconds,state:actor,contacts:contactSolving,
      height:height,normal:{x,z in ContactRig.surfaceNormal(x:x,z:z,height:height)})
  }
  func root(_ motion: MotionParameters) -> simd_float4x4 {
    mode == "bind"
      ? matrix_identity_float4x4
      : definition?.root(mode, seconds, actor, motion) ?? matrix_identity_float4x4
  }
  func visible(_ id: String, source: AssetSource) -> Bool {
    guard isolated, let selected else { return true }
    var key: String? = id
    while let k = key {
      if k == selected { return true }
      key = source.joints.first { $0.id == k }?.parent
    }
    return false
  }
  var telemetry: [String: Any] {
    [
      "mode": mode, "seconds": seconds, "rate": rate, "scenario": scenario,
      "state": actor.state, "reason": actor.reason, "fear": actor.fear,
      "position": [actor.position.x, actor.position.y], "target": [actor.target.x, actor.target.y],
      "hopping": actor.hopping, "phase": actor.phase as Any? ?? NSNull(), "seed": seed,
      "player": [input.player.x, input.player.y], "visible": input.visible,
      "running": input.running,
      "events": actor.events.map {
        ["tick": $0.tick as Any? ?? NSNull(), "state": $0.state, "reason": $0.reason]
          as [String: Any]
      },
    ]
  }
}

extension WorkshopRenderer {
  var selectedPartCenter: V3? {
    guard creatureWorkshop.focusPart, let selected = creatureWorkshop.selected,
      let part = studioSource.resolvedParts.first(where: { $0.key == selected }),
      let field = try? part.field.shape()
    else { return nil }
    let b = (try? studioSource.craft.anatomy.first(where:{$0.part==selected})?.shape().bounds) ?? field.bounds
    let m = creatureWorkshop.matrices(source: studioSource)[selected] ?? matrix_identity_float4x4
    let root = creatureWorkshop.root(studioSource.motion ?? MotionParameters())
    let p: SIMD4<Float> = simd_mul(simd_mul(root, m), SIMD4<Float>((b.min + b.max) / 2, 1))
    let center = (studioBounds.min + studioBounds.max) / 2
    return (V3(p.x, p.y, p.z) - V3(center.x, studioBounds.min.y, center.z)) * studioLayout.scale
  }
  func framePart() throws {
    guard let id = creatureWorkshop.selected,
      let part = studioSource.resolvedParts.first(where: { $0.key == id })
    else {
      throw RuntimeError.message("Select a part first")
    }
    let b = try studioSource.craft.anatomy.first(where:{$0.part==id})?.shape().bounds ?? part.field.shape().bounds
    creatureWorkshop.focusPart = true
    let size = length(b.max - b.min) * studioLayout.scale * (part.joint?.scale ?? 1)
    orbitDistance = boundedStudioDistance(max(0.05, size * 1.4) / studioUnit)
  }

  func editMotion(_ values: [String: Float]) throws {
    guard var motion = studioSource.motion else {
      throw RuntimeError.message("This asset has no motion recipe")
    }
    for (key, value) in values {
      guard let control = studioSource.animation?.controls.first(where: { $0.key == key }),
        value.isFinite, control.range.contains(value)
      else { throw RuntimeError.message("Invalid motion parameter: \(key)") }
      motion.values[key] = value
    }
    try studioSource.animation?.validate(motion)
    var candidate = studioSource
    candidate.motion = motion
    if var score=candidate.performance, let animation=candidate.animation {
      let oldDuration=animation.duration(studioSource.motion ?? MotionParameters())
      let ratio=animation.duration(motion)/oldDuration
      score.duration *= ratio
      for i in score.beats.indices {score.beats[i].time *= ratio}
      for i in score.tracks.indices {for k in score.tracks[i].curve.keys.indices {
        score.tracks[i].curve.keys[k].time *= ratio
        if let v=score.tracks[i].curve.keys[k].inTangent {score.tracks[i].curve.keys[k].inTangent=v/ratio}
        if let v=score.tracks[i].curve.keys[k].outTangent {score.tracks[i].curve.keys[k].outTangent=v/ratio}
      }}
      candidate.performance=score
    }
    if var arrangement=candidate.craft.arrangement,let animation=candidate.animation {
      let ratio=animation.duration(motion)/animation.duration(studioSource.motion ?? MotionParameters())
      arrangement.duration *= ratio
      for i in arrangement.layers.indices {
        arrangement.layers[i].start *= ratio;arrangement.layers[i].end *= ratio
        arrangement.layers[i].fadeIn *= ratio;arrangement.layers[i].fadeOut *= ratio
      }
      candidate.craft.arrangement=arrangement
    }
    try applySource(candidate)
    // A changed cadence must replay the brain against the new motion contract.
    try creatureWorkshop.seek(creatureWorkshop.seconds, motion: motion)
  }
  func editCreatureWorkshop(
    mode: String? = nil, scenario: String? = nil, seconds: Float? = nil,
    rate: Float? = nil, seed: UInt32? = nil, follow: Bool? = nil
  ) throws {
    var candidate = creatureWorkshop
    var restart = false
    if let mode, mode != candidate.mode {
      candidate.mode = mode
      restart = true
    }
    if let scenario {
      guard candidate.definition?.scenarios.contains(scenario) == true else {
        throw RuntimeError.message("Unknown behavior scenario")
      }
      candidate.scenario = scenario
      candidate.mode = "behavior"
      candidate.stimulusOverride = nil
      restart = true
    }
    if let rate { candidate.rate = rate }
    if let seed {
      candidate.seed = seed
      restart = true
    }
    if let follow { candidate.follow = follow }
    if restart { candidate.reset() }
    if let seconds {
      try candidate.seek(seconds, motion: studioSource.motion ?? MotionParameters())
    }
    try candidate.validate(source: studioSource)
    creatureWorkshop = candidate
    if follow != nil { fitStudioCamera() }
    if seconds != nil { paused = true }
  }
  func selectPart(_ id: String?, isolated: Bool? = nil) throws {
    var candidate = creatureWorkshop
    candidate.selected = id
    candidate.focusPart = false
    if let isolated { candidate.isolated = isolated }
    try candidate.validate(source: studioSource)
    creatureWorkshop = candidate
  }
  func editPart(_ values: [String: Float], parent: String? = nil) throws {
    guard let selected = creatureWorkshop.selected,
      let i = studioSource.resolvedParts.firstIndex(where: { $0.key == selected })
    else { throw RuntimeError.message("Select a field part first") }
    var source = studioSource
    var part = source.resolvedParts[i]
    let craftJoint=source.craft.joints.firstIndex{$0.id==selected}
    var joint = craftJoint.map{source.craft.joints[$0]} ?? part.joint ?? PartJoint(id: selected)
    if let parent { joint.parent = parent == "none" ? nil : parent }
    for (key, v) in values {
      guard v.isFinite else { throw RigError.invalid }
      switch key {
      case "x": joint.offset.x = v
      case "y": joint.offset.y = v
      case "z": joint.offset.z = v
      case "pivotX": joint.pivot.x = v
      case "pivotY": joint.pivot.y = v
      case "pivotZ": joint.pivot.z = v
      case "pitch": joint.rotation.x = v
      case "yaw": joint.rotation.y = v
      case "roll": joint.rotation.z = v
      case "scale": joint.scale = v
      case "roughness": part.roughness = v
      case "metallic": part.metallic = v
      default: throw RuntimeError.message("Unknown part parameter \(key)")
      }
    }
    let ownsBase=source.generator=="fields" && i<source.parts.count
    var override=source.partOverrides[selected] ?? PartOverride(),changedOverride=false
    if parent != nil || values.keys.contains(where:{!["roughness","metallic"].contains($0)}) {
      if let craftJoint {source.craft.joints[craftJoint]=joint}
      else if ownsBase {source.parts[i].joint=joint}
      else {override.joint=joint;changedOverride=true}
    }
    let anatomy=source.craft.anatomy.firstIndex{$0.part==selected}
    if values["roughness"] != nil {
      if let anatomy {source.craft.anatomy[anatomy].roughness=part.roughness}
      else if ownsBase {source.parts[i].roughness=part.roughness}
      else {override.roughness=part.roughness;changedOverride=true}
    }
    if values["metallic"] != nil {
      if let anatomy {source.craft.anatomy[anatomy].metallic=part.metallic}
      else if ownsBase {source.parts[i].metallic=part.metallic}
      else {override.metallic=part.metallic;changedOverride=true}
    }
    if changedOverride {source.partOverrides[selected]=override}
    try applySource(source)
  }
}

extension CreatureWorkshop {
  enum CodingKeys: String, CodingKey {
    case rehearsal, contactView, contactSolving, rigView, recording, mode, scenario, generator, authoredAnimation, seconds, rate, remainder, seed, actor, follow,
      stimulusOverride,
      selected, isolated, focusPart
  }
  init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    rehearsal = try c.decodeIfPresent(RehearsalSurface.self,forKey:.rehearsal) ?? RehearsalSurface()
    contactView = try c.decodeIfPresent(Bool.self,forKey:.contactView) ?? false
    contactSolving = try c.decodeIfPresent(Bool.self,forKey:.contactSolving) ?? true
    rigView = try c.decodeIfPresent(Bool.self, forKey:.rigView) ?? false
    recording = try c.decodeIfPresent(String.self, forKey: .recording)
    mode = try c.decodeIfPresent(String.self, forKey: .mode) ?? "bind"
    scenario = try c.decodeIfPresent(String.self, forKey: .scenario) ?? ""
    generator = try c.decodeIfPresent(String.self, forKey: .generator) ?? ""
    authoredAnimation = try c.decodeIfPresent(AuthoredAnimation.self,forKey:.authoredAnimation)
    seconds = try c.decodeIfPresent(Float.self, forKey: .seconds) ?? 0
    rate = try c.decodeIfPresent(Float.self, forKey: .rate) ?? 1
    remainder = try c.decodeIfPresent(Float.self, forKey: .remainder) ?? 0
    seed = try c.decodeIfPresent(UInt32.self, forKey: .seed) ?? 17
    actor = try c.decodeIfPresent(BehaviorSnapshot.self, forKey: .actor) ?? BehaviorSnapshot()
    follow = try c.decodeIfPresent(Bool.self, forKey: .follow) ?? true
    stimulusOverride = try c.decodeIfPresent(PreviewInput.self, forKey: .stimulusOverride)
    selected = try c.decodeIfPresent(String.self, forKey: .selected)
    isolated = try c.decodeIfPresent(Bool.self, forKey: .isolated) ?? false
    focusPart = try c.decodeIfPresent(Bool.self, forKey: .focusPart) ?? false
  }
}
