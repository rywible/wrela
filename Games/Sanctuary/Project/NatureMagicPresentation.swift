import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import SimulationCore
import simd

/// A one-second authored response to a saved nature contribution. It changes presentation
/// only: habitat, water, collision and terrain take effect through their production owners.
/// The same pure poses drive the native garden and the registered Soundstage subjects.
final class NatureMagicPresentation {
  static let duration: Double = 1
  static let maximumRipplePatches = 4

  struct Event {
    let patchID: UInt64
    let createdAtPlaySeconds: Double?
    /// Actual elapsed saved play time, before any per-element stagger or pose clamping.
    let elapsedMilliseconds: Double?
    let seed: UInt64

    var active: Bool {
      elapsedMilliseconds.map { $0 >= 0 && $0 < NatureMagicPresentation.duration * 1_000 } ?? false
    }

    var diagnostics: [String: Any] {
      var result: [String: Any] = [
        "patchID": String(patchID), "seed": String(seed), "active": active,
        "durationMilliseconds": NatureMagicPresentation.duration * 1_000,
      ]
      if let createdAtPlaySeconds { result["createdAtPlaySeconds"] = createdAtPlaySeconds }
      if let elapsedMilliseconds { result["elapsedMilliseconds"] = elapsedMilliseconds }
      return result
    }
  }

  struct RipplePose {
    let active: Bool
    let pose: JointPose
  }

  private let ring: GPUBatch

  init(graphics: MetalRenderer) {
    ring = graphics.upload(Self.ringBatch(name: Self.ringNames[0]))
  }

  static func event(
    patchID: UInt64, createdAtPlaySeconds: Double?, playSeconds: Double
  ) -> Event {
    let elapsed: Double?
    if let createdAtPlaySeconds, createdAtPlaySeconds.isFinite,
      playSeconds.isFinite, (0...HabitatGarden.maximumPlaySeconds).contains(playSeconds),
      createdAtPlaySeconds >= 0, playSeconds >= createdAtPlaySeconds
    {
      // Subtract in Double. Long-running saves must not lose the first animation frames
      // by converting two large absolute play times to Float before subtracting them.
      elapsed = (playSeconds - createdAtPlaySeconds) * 1_000
    } else { elapsed = nil }
    return Event(patchID: patchID, createdAtPlaySeconds: createdAtPlaySeconds,
      elapsedMilliseconds: elapsed, seed: mixed(patchID ^ 0xE703_7ED1_A0B4_28DB))
  }

  static func event(for patch: HabitatGarden.Patch, playSeconds: Double) -> Event {
    event(patchID: patch.id, createdAtPlaySeconds: patch.createdAtPlaySeconds,
      playSeconds: playSeconds)
  }

  /// Read-only telemetry uses the same saved timestamps and exact seed as the rendered pose.
  static func activeEvents(garden: HabitatGarden, playSeconds: Double) -> [Event] {
    garden.patches.map { event(for: $0, playSeconds: playSeconds) }.filter(\.active)
  }

  private static func mixed(_ value: UInt64) -> UInt64 {
    var x = value &+ 0x9E37_79B9_7F4A_7C15
    x = (x ^ (x >> 30)) &* 0xBF58_476D_1CE4_E5B9
    x = (x ^ (x >> 27)) &* 0x94D0_49BB_1331_11EB
    return x ^ (x >> 31)
  }

  private static func smooth(_ value: Float) -> Float {
    let t = min(1, max(0, value))
    return t * t * (3 - 2 * t)
  }

  static func growthPose(event: Event, elementIndex: Int) -> JointPose {
    guard event.active, let milliseconds = event.elapsedMilliseconds else { return JointPose() }
    let elementSeed = mixed(event.seed &+ UInt64(max(0, elementIndex)))
    let delay = Float(elementSeed & 1023) / 1023 * 0.12
    let elapsed = Float(milliseconds / 1_000)
    let t = min(1, max(0, (elapsed - delay) / (1 - delay)))
    // Zero initial speed, one restrained overshoot, then an exact settled endpoint.
    let spring = 1 - exp(-8 * t) * (cos(10 * t) + 0.8 * sin(10 * t))
    let settle = smooth((t - 0.78) / 0.22)
    let height = max(0.025, min(1.065, spring * (1 - settle) + settle))
    let breadth = 0.16 + 0.84 * smooth(t / 0.58)
    let angle = Float((elementSeed >> 16) & 65535) / 65535 * 2 * Float.pi
    let tilt = 7 * exp(-6 * t) * sin(11 * t) * (1 - settle)
    // The ground pivot never translates upward: growth springs from its contact point.
    return JointPose(rotation: V3(cos(angle) * tilt, 0, sin(angle) * tilt),
      scale: V3(breadth, height, breadth))
  }

  static func growthTransform(event: Event, elementIndex: Int) -> simd_float4x4 {
    guard event.active else { return matrix_identity_float4x4 }
    return matrix(growthPose(event: event, elementIndex: elementIndex))
  }

  static func ripplePose(event: Event, ringIndex: Int, radius: Float) -> RipplePose {
    let hidden = RipplePose(active: false,
      pose: JointPose(offset: V3(0, -0.03, 0), scale: V3(repeating: 0.0001)))
    guard event.active, let milliseconds = event.elapsedMilliseconds,
      radius.isFinite, radius > 0, (0...1).contains(ringIndex)
    else { return hidden }
    let delay: Float = ringIndex == 0 ? 0.015 : 0.16
    let elapsed = Float(milliseconds / 1_000)
    guard elapsed >= delay else { return hidden }
    let t = min(1, max(0, (elapsed - delay) / (1 - delay)))
    let reach = min(1.4, radius) * (0.16 + 0.84 * smooth(t))
    let sink = smooth((t - 0.66) / 0.34)
    // Thin crests settle back under the supplied water surface; no alpha blend or glow
    // pipeline is needed, and a completed invitation emits no production render items.
    return RipplePose(active: true,
      pose: JointPose(offset: V3(0, 0.022 * (1 - sink) - 0.04 * sink, 0),
        scale: V3(reach, max(0.02, 1 - sink), reach)))
  }

  static func matrix(_ pose: JointPose) -> simd_float4x4 {
    let radians = pose.rotation * (.pi / 180)
    let rotation = simd_quatf(angle: radians.z, axis: V3(0, 0, 1))
      * simd_quatf(angle: radians.y, axis: V3(0, 1, 0))
      * simd_quatf(angle: radians.x, axis: V3(1, 0, 0))
    return transform(pose.offset, V3(repeating: 1), 0) * simd_float4x4(rotation)
      * simd_float4x4(diagonal: SIMD4(pose.scale, 1))
  }

  func items(
    garden: HabitatGarden, playSeconds: Double, player: PlayerCamera,
    waterHeight: (Float, Float) -> Float?
  ) -> [RenderItem] {
    let playerXZ = SIMD2(player.position.x, player.position.z)
    let patches = garden.patches.filter {
      $0.planting == .shallowWater && Self.event(for: $0, playSeconds: playSeconds).active
        && distance(SIMD2($0.center.x, $0.center.z), playerXZ) <= 80
    }.sorted {
      let a = distance_squared(SIMD2($0.center.x, $0.center.z), playerXZ)
      let b = distance_squared(SIMD2($1.center.x, $1.center.z), playerXZ)
      return a == b ? $0.id < $1.id : a < b
    }.prefix(Self.maximumRipplePatches)
    var result: [RenderItem] = []
    for patch in patches {
      let x = patch.center.x, z = patch.center.z
      guard let height = waterHeight(x, z), height.isFinite else { continue }
      let e: Float = 0.08
      func sample(_ px: Float, _ pz: Float) -> Float {
        guard let value = waterHeight(px, pz), value.isFinite else { return height }
        return value
      }
      let dx = sample(x + e, z) - sample(x - e, z)
      let dz = sample(x, z + e) - sample(x, z - e)
      let normal = normalize(V3(-dx, 2 * e, -dz))
      let contact = transform(V3(x, height, z), V3(repeating: 1), 0)
        * simd_float4x4(simd_quatf(from: V3(0, 1, 0), to: normal))
      let event = Self.event(for: patch, playSeconds: playSeconds)
      for index in 0..<2 {
        let frame = Self.ripplePose(event: event, ringIndex: index, radius: patch.radius * 0.55)
        guard frame.active else { continue }
        var instance = ring.sourceInstances[0]
        instance.model = contact * Self.matrix(frame.pose) * instance.model
        result.append(RenderItem(batch: ring, instance: instance, castsShadow: false))
      }
    }
    return result
  }

  private static let growthName = "Nature growth plant"
  private static let ringNames = ["Nature invitation first", "Nature invitation second"]

  /// One 512-triangle torus is uploaded once and reused for every invitation ring.
  private static func ringMesh() -> Mesh {
    ParametricMesh.surface(u: 64, v: 4, color: V3(0.47, 0.66, 0.62)) { u, v in
      let angle = u * 2 * Float.pi, tube = v * 2 * Float.pi
      let radius = 1 + 0.008 * cos(tube)
      return V3(radius * cos(angle), -0.008 * sin(tube), radius * sin(angle))
    }
  }

  private static func ringBatch(name: String) -> SceneBatch {
    SceneBatch(name: name, mesh: ringMesh(), instances: [Instance(kind: 7)],
      roughness: 0.28, doubleSided: true, metallic: 0.02)
  }

  static var generators: [AssetGenerator] {
    [false, true].map { water in
      let names = water ? ringNames : [growthName]
      return AssetGenerator(
        id: water ? "nature-water-invitation" : "nature-growth",
        name: water ? "Nature · Water invitation" : "Nature · Grounded growth",
        animation: animation(water: water),
        parts: { _ in names.map { name in
          AssetPart(name: name, joint: PartJoint(id: name),
            field: .ellipsoid(V3(0, 0.2, 0), V3(0.2, 0.2, 0.2)), color: [1, 1, 1], material: 7)
        } },
        compile: { _ in
          if water { return names.map { ringBatch(name: $0) } }
          return [SceneBatch(name: growthName, mesh: SanctuaryProceduralCraft.flower(),
            instances: [Instance(kind: 7)], roughness: 0.87, doubleSided: true)]
        })
    }
  }

  private static func studyEvent(
    clip: String, time: Float, state: BehaviorSnapshot, parameters: MotionParameters
  ) -> Event {
    let id = UInt64(parameters.values["patchID"] ?? 37)
    let elapsed = clip == "settled" ? 1.0 : Double(clip == "behavior" ? Float(state.tick) / 60 : time)
    return event(patchID: id, createdAtPlaySeconds: 0, playSeconds: max(0, elapsed))
  }

  private static func animation(water: Bool) -> AnimationDefinition {
    let controls: [ScalarControl] = [
      .init("patchID", "Saved patch ID", 37, 1...16_777_215),
      water ? .init("invitationRadius", "Invitation radius · m", 1, 0.1...1.4)
        : .init("elementIndex", "Plant index in patch", 0, 0...35),
    ]
    let envelope = water
      ? Bounds(V3(-1.6, -0.06, -1.6), V3(1.6, 0.08, 1.6))
      : Bounds(V3(-0.25, -0.02, -0.25), V3(0.25, 0.55, 0.25))
    var result = AnimationDefinition(
      controls: controls, clips: [water ? "invite" : "grow", "settled"],
      scenarios: ["rehearsal"], signalName: "Progress", duration: { _ in 1.35 },
      poses: { clip, time, state, parameters in
        let event = studyEvent(clip: clip, time: time, state: state, parameters: parameters)
        if water {
          return Dictionary(uniqueKeysWithValues: ringNames.enumerated().map { index, name in
            (name, ripplePose(event: event, ringIndex: index,
              radius: parameters.values["invitationRadius"] ?? 1).pose)
          })
        }
        return [growthName: growthPose(event: event,
          elementIndex: Int(parameters.values["elementIndex"] ?? 0))]
      }, root: { _, _, _, _ in matrix_identity_float4x4 },
      initialize: { _ in
        var state = BehaviorSnapshot()
        state.state = "inviting"; state.reason = "Saved-time nature gesture"
        return state
      }, step: { state, _, _ in
        var state = state
        state.tick = min(81, state.tick + 1)
        state.state = state.tick < 60 ? "inviting" : "settled"
        state.fear = min(1, Float(state.tick) / 60)
        return state
      }, input: { _, _ in PreviewInput() },
      bounds: { _ in envelope },
      validate: { parameters in
        for control in controls {
          let value = parameters.values[control.key] ?? control.initial
          guard value.isFinite, control.range.contains(value) else {
            throw RuntimeError.message("Invalid nature gesture control: \(control.key)")
          }
        }
      })
    result.inspect = { clip, time, state, parameters in
      let event = studyEvent(clip: clip, time: time, state: state, parameters: parameters)
      let pose = growthPose(event: event, elementIndex: Int(parameters.values["elementIndex"] ?? 0))
      return ["elapsedMilliseconds": Float(event.elapsedMilliseconds ?? 1_000),
        "savedPatchID": Float(event.patchID), "active": event.active ? 1 : 0,
        "growthHeight": pose.scale.y,
        "seedWord0": Float(event.seed & 65535),
        "seedWord1": Float((event.seed >> 16) & 65535),
        "seedWord2": Float((event.seed >> 32) & 65535),
        "seedWord3": Float((event.seed >> 48) & 65535)]
    }
    result.motionEnvelope = envelope
    return result
  }
}
