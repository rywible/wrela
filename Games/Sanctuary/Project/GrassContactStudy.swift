import FieldCompiler
import FieldCore
import FieldEngine
import simd

/// A repeatable authoring fixture for the production grass representation and
/// shared contact field. It creates source contacts, never a second deformation.
enum GrassContactStudy {
  private static let epoch: Double = 999_999_000
  private static let sweepSeconds: Double = 2
  private static let segments = 32

  static var generator: AssetGenerator {
    AssetGenerator(id: "grass-contact", name: "Grass · Contact and recovery",
      animation: animation, compile: { _ in [batch()] })
  }

  private static func batch() -> SceneBatch {
    var random = SeededRandom(seed: 7_319)
    var blades: [GrassBlade] = []
    // Stratified placement preserves gaps without a conspicuous regular row.
    for z in 0..<22 {
      for x in 0..<22 {
        let anchor = V3(-1 + (Float(x) + random.range(0.15, 0.85)) / 11, 0,
          -1 + (Float(z) + random.range(0.15, 0.85)) / 11)
        blades.append(GrassBlade(anchor: anchor, angle: random.range(0, 2 * .pi),
          height: random.range(0.24, 0.43), width: random.range(0.012, 0.022),
          color: V3(random.range(0.30, 0.44), random.range(0.45, 0.58), random.range(0.18, 0.28))))
      }
    }
    return SceneBatch(name: "Contact meadow", mesh: Mesh(),
      instances: [Instance(kind: 3)], grass: blades, roughness: 0.85, doubleSided: true)
  }

  private static func elapsed(_ clip: String, _ seconds: Float,
    _ state: BehaviorSnapshot, _ p: MotionParameters) -> Double
  {
    let recovery = Double(p.values["recoverySeconds"] ?? 1.1)
    if clip == "settled" { return sweepSeconds + recovery * 8 }
    let time = Double(clip == "behavior" ? Float(state.tick) / 60 : seconds)
    return max(0, time) + (clip == "recover" ? sweepSeconds : 0)
  }

  private static func snapshot(_ clip: String, _ seconds: Float,
    _ state: BehaviorSnapshot, _ p: MotionParameters,
    _ placements: [simd_float4x4]) -> SurfaceInfluenceSnapshot
  {
    let time = elapsed(clip, seconds, state, p)
    let contactHeight: Float = clip == "support-height" ? 1 : 0
    var events: [GroundInfluenceEvent] = []
    // Three normal studio placements use at most 96 events. The explicit cap
    // also bounds this source if a future caller supplies additional placements.
    for (placementIndex, model) in placements.prefix(8).enumerated() {
      let scale = length(V3(model[0].x, model[0].y, model[0].z))
      let direction = model * SIMD4<Float>(1, 0, 0, 0)
      let heading = normalize(SIMD2(direction.x, direction.z))
      func point(_ t: Double) -> V3 {
        let result = model * SIMD4<Float>(-0.85 + 1.7 * Float(t / sweepSeconds), contactHeight, 0, 1)
        return V3(result.x, result.y, result.z)
      }
      for segment in 0..<segments {
        let begin = Double(segment) * sweepSeconds / Double(segments)
        guard begin <= time else { break }
        let end = min(time, Double(segment + 1) * sweepSeconds / Double(segments))
        events.append(GroundInfluenceEvent(id: UInt64(placementIndex * segments + segment + 1),
          sourceID: "grass-study-\(placementIndex)", kind: .body,
          start: point(begin), end: point(end), radius: (p.values["radius"] ?? 0.28) * scale,
          displacement: (p.values["displacement"] ?? 0.2) * scale,
          compression: p.values["compression"] ?? 0.65, supportTolerance: 0.2 * scale,
          heading: heading, startTime: epoch + begin, endTime: epoch + end,
          recoverySeconds: p.values["recoverySeconds"] ?? 1.1))
      }
    }
    return SurfaceInfluenceSnapshot(time: epoch + time, events: events)
  }

  private static var animation: AnimationDefinition {
    // Dimensions remain valid throughout the studio's 0.25...4 instance scale.
    let controls: [ScalarControl] = [
      .init("radius", "Contact radius · m", 0.28, 0.2...0.45),
      .init("displacement", "Bend displacement · m", 0.2, 0...0.35),
      .init("compression", "Compression", 0.65, 0...1),
      .init("recoverySeconds", "Recovery time scale · s", 1.1, 0.25...4),
    ]
    var definition = AnimationDefinition(controls: controls,
      clips: ["sweep", "recover", "settled", "support-height"], scenarios: ["contact-sweep"],
      signalName: "Contact", duration: { 2 + ($0.values["recoverySeconds"] ?? 1.1) * 8 },
      poses: { _, _, _, _ in [:] }, root: { _, _, _, _ in matrix_identity_float4x4 },
      initialize: { _ in BehaviorSnapshot() }, step: { state, _, _ in
        var next = state
        next.tick = min(3_600, next.tick + 1)
        next.state = next.tick <= 120 ? "sweeping" : "recovering"
        return next
      }, input: { _, _ in PreviewInput() },
      bounds: { _ in Bounds(V3(-1.1, 0, -1.1), V3(1.1, 0.5, 1.1)) },
      validate: { p in
        for control in controls {
          let value = p.values[control.key] ?? control.initial
          guard value.isFinite, control.range.contains(value) else {
            throw RuntimeError.message("Invalid grass contact control: \(control.key)")
          }
        }
      })
    definition.surfaceInfluences = snapshot
    definition.inspect = { clip, seconds, state, p in
      let source = snapshot(clip, seconds, state, p, [matrix_identity_float4x4])
      return ["sourceBlades": 484, "sourceEvents": Float(source.events.count),
        "contactElapsedSeconds": Float(source.time - epoch),
        "maximumSharedResponse": source.events.map { $0.response(at: source.time) }.max() ?? 0,
        "supportHeightMetres": clip == "support-height" ? 1 : 0]
    }
    return definition
  }
}
