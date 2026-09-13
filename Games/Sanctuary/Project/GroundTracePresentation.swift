import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import simd

/// Source-authored temporary soil impressions. The game supplies actual foot contacts and
/// eligible supporting soil; this adapter never infers footsteps from body sweeps or water.
/// Raised rims/darker centers approximate relief without displacing the terrain or collision.
final class GroundTracePresentation {
  enum FootShape: String, CaseIterable { case boot, softContact }

  static let defaultSoilColor = V3(0.32, 0.29, 0.23)
  static let maximumTraces = 64

  struct Support {
    let height: Float
    let soilColor: V3
    let strength: Float
    let shape: FootShape

    init(height: Float, soilColor: V3 = GroundTracePresentation.defaultSoilColor,
      strength: Float = 1, shape: FootShape = .boot)
    {
      self.height = height; self.soilColor = soilColor
      self.strength = strength; self.shape = shape
    }

    var valid: Bool {
      height.isFinite && abs(height) <= 1_000_000 && strength.isFinite
        && (0...1).contains(strength)
        && [soilColor.x, soilColor.y, soilColor.z].allSatisfy { $0.isFinite && (0...1).contains($0) }
    }
  }

  private struct Prepared {
    let support: Support
    let placement: simd_float4x4
    /// Nine local affine tangent planes. Local X/Z are scaled to metres by placement;
    /// local Y is already metres, so terrain slopes and animated relief compose directly.
    let planes: [simd_float4x4]
  }

  private struct Cached {
    let event: GroundInfluenceEvent
    let supportRevision: UInt64
    let prepared: Prepared?
  }

  private let batches: [FootShape: GPUBatch]
  private var cache: [UInt64: Cached] = [:]
  private var lastTime: Double?
  private var lastRevision: UInt64?
  private(set) var supportQueriesLastFrame = 0
  private(set) var renderedTraceCount = 0
  var cachedEventCount: Int { cache.count }

  init(graphics: MetalRenderer) {
    batches = Dictionary(uniqueKeysWithValues: FootShape.allCases.map {
      ($0, graphics.upload(Self.batch(shape: $0)))
    })
  }

  /// Call on explicit snapshot replacement. Backward saved time and support revisions also
  /// invalidate automatically; ordinary advancing snapshots keep their support cache.
  func reset() {
    cache.removeAll(keepingCapacity: true)
    lastTime = nil; lastRevision = nil
    supportQueriesLastFrame = 0; renderedTraceCount = 0
  }

  func items(
    snapshot: SurfaceInfluenceSnapshot, playerPosition: V3, supportRevision: UInt64,
    support: (GroundInfluenceEvent, Float, Float) -> Support?
  ) -> [RenderItem] {
    guard snapshot.time.isFinite, (0...1_000_000_000).contains(snapshot.time),
      [playerPosition.x, playerPosition.y, playerPosition.z].allSatisfy(\.isFinite)
    else { reset(); return [] }
    if lastTime.map({ snapshot.time < $0 }) ?? false
      || lastRevision.map({ $0 != supportRevision }) ?? false
    { cache.removeAll(keepingCapacity: true) }
    lastTime = snapshot.time; lastRevision = supportRevision
    supportQueriesLastFrame = 0; renderedTraceCount = 0
    let candidates = snapshot.events.filter {
      $0.kind == .foot && distance_squared($0.start, $0.end) <= 0.0004
        && $0.response(at: snapshot.time) > 0.0001
        && distance_squared($0.end, playerPosition) < 28 * 28
    }.sorted {
      let a = distance_squared($0.end, playerPosition), b = distance_squared($1.end, playerPosition)
      return a == b ? $0.id < $1.id : a < b
    }.prefix(Self.maximumTraces)
    let retained = Set(candidates.map(\.id))
    cache = cache.filter { retained.contains($0.key) }
    var result: [RenderItem] = []
    for event in candidates {
      let prepared: Prepared?
      if let existing = cache[event.id], existing.event == event,
        existing.supportRevision == supportRevision
      { prepared = existing.prepared }
      else {
        prepared = prepare(event: event, support: support)
        // Rejected contacts are cached too: unchanged water/building/soil rejections should
        // not trigger dozens of repeated terrain queries every rendered frame.
        cache[event.id] = Cached(event: event, supportRevision: supportRevision, prepared: prepared)
      }
      guard let prepared, let batch = batches[prepared.support.shape] else { continue }
      let pose = Self.recoveryPose(event: event, time: snapshot.time, strength: prepared.support.strength)
      let recovery = transform(pose.offset, pose.scale, 0)
      var instance = batch.sourceInstances[0]
      instance.model = prepared.placement
      instance.tint = SIMD4(prepared.support.soilColor, 7)
      var item = RenderItem(batch: batch, instance: instance, castsShadow: false)
      item.skinPalette = prepared.planes.map { $0 * recovery }
      result.append(item)
    }
    renderedTraceCount = result.count
    return result
  }

  private func prepare(
    event: GroundInfluenceEvent, support: (GroundInfluenceEvent, Float, Float) -> Support?
  ) -> Prepared? {
    guard (try? event.validate()) != nil, event.kind == .foot else { return nil }
    supportQueriesLastFrame += 1
    guard let center = support(event, event.end.x, event.end.z), center.valid,
      center.strength > 0,
      abs(center.height - event.end.y) <= event.supportTolerance
    else { return nil }
    let heading = normalize(event.heading)
    let yaw = atan2(-heading.x, -heading.y)
    let placement = transform(V3(event.end.x, center.height, event.end.z),
      V3(event.radius, 1, event.radius), yaw)
    var heights = [Float](repeating: center.height, count: 9)
    for index in 0..<9 where index != 4 {
      let local = Self.grid[index]
      let position = placement * SIMD4(local.x, 0, local.y, 1)
      supportQueriesLastFrame += 1
      guard let sample = support(event, position.x, position.z), sample.valid,
        sample.strength > 0, sample.shape == center.shape,
        abs(sample.height - event.end.y) <= event.supportTolerance
      else { return nil }
      heights[index] = sample.height
    }
    var planes: [simd_float4x4] = []
    for z in 0..<3 {
      for x in 0..<3 {
        let id = z * 3 + x
        let xl = max(0, x - 1), xr = min(2, x + 1)
        let zl = max(0, z - 1), zr = min(2, z + 1)
        let gx = (heights[z * 3 + xr] - heights[z * 3 + xl]) / (Float(xr - xl) * 0.7)
        let gz = (heights[zr * 3 + x] - heights[zl * 3 + x]) / (Float(zr - zl) * 1.2)
        var matrix = matrix_identity_float4x4
        matrix.columns.0.y = gx; matrix.columns.2.y = gz
        matrix.columns.3.y = heights[id] - center.height - gx * Self.grid[id].x - gz * Self.grid[id].y
        planes.append(matrix)
      }
    }
    return Prepared(support: center, placement: placement, planes: planes)
  }

  /// Both game and Soundstage use the shared contact response and its saved Double time.
  /// Keep the compacted center clear of depth fighting while the rim relaxes. The last
  /// response tail contracts and buries the residual mark; zero response has zero area,
  /// including in a studio with its ground hidden. No clock or visibility state is stored.
  static func recoveryPose(event: GroundInfluenceEvent, time: Double, strength: Float) -> JointPose {
    let response = event.response(at: time)
    let amount = min(1, max(0, response * event.compression * strength))
    let tail = min(1, amount / 0.18)
    let coverage = tail * tail * (3 - 2 * tail)
    return JointPose(offset: V3(0, 0.0008 * (1 - amount) - 0.002 * (1 - coverage), 0),
      scale: V3(coverage, max(0.0001, amount), coverage))
  }

  private static let grid: [SIMD2<Float>] = (0..<3).flatMap { z in
    (0..<3).map { x in SIMD2(Float(x - 1) * 0.7, Float(z - 1) * 1.2) }
  }
  private static let joints = (0..<9).map { "Soil support \($0)" }

  private static func skin(_ p: V3) -> SkinWeight {
    let x = min(1.99999, max(0, (p.x + 0.7) / 0.7))
    let z = min(1.99999, max(0, (p.z + 1.2) / 1.2))
    let ix = Int(x), iz = Int(z), fx = x - Float(ix), fz = z - Float(iz)
    let a = UInt32(iz * 3 + ix)
    var weight = SkinWeight(a)
    weight.joints = SIMD4(a, a + 1, a + 3, a + 4)
    weight.weights = SIMD4((1 - fx) * (1 - fz), fx * (1 - fz), (1 - fx) * fz, fx * fz)
    return weight
  }

  /// Near-flush compacted soil with a tiny, interrupted edge, not a solid raised sole.
  /// Bind-space min Y is exactly zero: Soundstage's automatic grounding therefore adds
  /// no lift relative to the game's supporting plane. Negative outer bind vertices made
  /// the first candidate hover in the studio even after its recovery lowered the center.
  /// This surface still cannot expose a true hole through the unchanged base terrain.
  private static func impression(
    center: SIMD2<Float>, radius: SIMD2<Float>, angles: Int, boot: Bool
  ) -> Mesh {
    let rings = boot ? 5 : 4
    var points = [V3(center.x, 0.0008, center.y)]
    var colors = [V3(repeating: 0.71)]
    for ring in 1...rings {
      let r = Float(ring) / Float(rings)
      for segment in 0..<angles {
        let a = Float(segment) / Float(angles) * 2 * Float.pi
        let z = sin(a)
        let waist: Float = boot ? (1 - 0.28 * exp(-pow((z - 0.18) / 0.30, 2))) : 1
        let toe: Float = boot ? (1 - z * 0.12) : 1
        let uneven = 1 + 0.025 * sin(7 * a + 0.6) + 0.015 * cos(11 * a - 0.2)
        let point = center + radius * SIMD2(cos(a) * waist * toe, z) * r * uneven
        let edge = max(0, (r - 0.82) / 0.18)
        // Broken pressure ridges avoid the continuous rubber/slipper outline. The full
        // rim is bounded to 2 mm before compression, independent of footprint radius.
        let fragments = max(0, sin(5 * a + 0.4) * 0.65 + cos(9 * a - 0.7) * 0.35)
        let ridge = exp(-pow((r - 0.79) / 0.12, 2)) * fragments
        let height: Float = ring == rings ? 0 : 0.0008 * (1 - edge) + 0.0012 * ridge
        points.append(V3(point.x, height, point.y))
        let pressure = boot ? exp(-pow((point.y - 0.20) / 0.12, 2)) * 0.08 : 0
        let grain = sin(a * 13 + r * 9) * 0.025
        colors.append(V3(repeating: min(1, 0.71 + pressure + 0.22 * pow(r, 3) + grain)))
      }
    }
    var faces: [(Int, Int, Int)] = []
    for i in 0..<angles { faces.append((0, 1 + (i + 1) % angles, 1 + i)) }
    for ring in 0..<(rings - 1) {
      for i in 0..<angles {
        let a = 1 + ring * angles + i, b = 1 + ring * angles + (i + 1) % angles
        let c = a + angles, d = b + angles
        faces += [(a, b, c), (b, d, c)]
      }
    }
    var normals = [V3](repeating: .zero, count: points.count)
    for (a, b, c) in faces {
      let normal = cross(points[b] - points[a], points[c] - points[a])
      normals[a] += normal; normals[b] += normal; normals[c] += normal
    }
    var mesh = Mesh()
    mesh.vertices = points.indices.map { Vertex(points[$0], normalize(normals[$0]), colors[$0]) }
    mesh.indices = faces.flatMap { [UInt32($0.0), UInt32($0.1), UInt32($0.2)] }
    return mesh
  }

  private static func mesh(shape: FootShape) -> Mesh {
    switch shape {
    case .boot:
      return impression(center: .zero, radius: SIMD2(0.55, 1.15), angles: 48, boot: true)
    case .softContact:
      var pieces = [impression(center: SIMD2(0, 0.22), radius: SIMD2(0.43, 0.55), angles: 24, boot: false)]
      for i in 0..<3 {
        pieces.append(impression(center: SIMD2(Float(i - 1) * 0.34, i == 1 ? -0.77 : -0.59),
          radius: SIMD2(0.19, 0.23), angles: 24, boot: false))
      }
      return ParametricMesh.joined(pieces)
    }
  }

  private static func batch(shape: FootShape) -> SceneBatch {
    let mesh = mesh(shape: shape)
    var result = SceneBatch(name: "Ground \(shape.rawValue) impression", mesh: mesh,
      instances: [Instance(tint: defaultSoilColor, kind: 7)], roughness: 0.97, doubleSided: true)
    result.skinJoints = joints
    result.skinWeights = mesh.vertices.map { skin(V3($0.position.x, $0.position.y, $0.position.z)) }
    return result
  }

  static var generators: [AssetGenerator] {
    FootShape.allCases.map { shape in
      AssetGenerator(id: shape == .boot ? "ground-boot-trace" : "ground-soft-trace",
        name: shape == .boot ? "Ground · Boot impression" : "Ground · Soft contact impression",
        animation: animation(shape: shape),
        parts: { _ in joints.map {
          AssetPart(name: $0, joint: PartJoint(id: $0),
            field: .ellipsoid(.zero, V3(0.1, 0.01, 0.1)), color: [1, 1, 1], material: 7)
        } }, compile: { _ in [batch(shape: shape)] })
    }
  }

  private static func studyEvent(_ p: MotionParameters) -> GroundInfluenceEvent {
    GroundInfluenceEvent(id: 1, sourceID: "authored-contact", kind: .foot,
      start: .zero, end: .zero, radius: p.values["radius"] ?? 0.13,
      displacement: 0.025, compression: p.values["compression"] ?? 0.72,
      heading: SIMD2(0, -1), startTime: 0, endTime: 0,
      recoverySeconds: p.values["recoverySeconds"] ?? 3)
  }

  private static func animation(shape: FootShape) -> AnimationDefinition {
    let controls: [ScalarControl] = [
      .init("radius", "Contact radius · m", shape == .boot ? 0.13 : 0.11, 0.05...2),
      .init("compression", "Soil compression", 0.72, 0...1),
      .init("recoverySeconds", "Recovery time scale · s", 3, 0.1...7),
    ]
    func elapsed(_ clip: String, _ time: Float, _ state: BehaviorSnapshot, _ p: MotionParameters) -> Double {
      clip == "settled" ? Double(studyEvent(p).recoverySeconds) * 8
        : Double(clip == "behavior" ? Float(state.tick) / 60 : time)
    }
    var definition = AnimationDefinition(controls: controls, clips: ["recover", "settled"],
      scenarios: ["rehearsal"], signalName: "Compression", duration: { ($0.values["recoverySeconds"] ?? 3) * 8 + 0.2 },
      poses: { clip, time, state, p in
        let pose = recoveryPose(event: studyEvent(p), time: elapsed(clip, time, state, p), strength: 1)
        return Dictionary(uniqueKeysWithValues: joints.map { ($0, pose) })
      }, root: { _, _, _, p in
        let radius = p.values["radius"] ?? (shape == .boot ? 0.13 : 0.11)
        return transform(.zero, V3(radius, 1, radius), 0)
      }, initialize: { _ in BehaviorSnapshot() }, step: { state, _, p in
        var state = state
        state.tick = min(3_400, state.tick + 1)
        state.fear = studyEvent(p).response(at: Double(state.tick) / 60)
        state.state = state.fear > 0 ? "recovering" : "settled"
        return state
      }, input: { _, _ in PreviewInput() },
      bounds: { _ in Bounds(V3(-1.4, -0.02, -2.4), V3(1.4, 0.03, 2.4)) },
      validate: { p in
        for control in controls {
          let value = p.values[control.key] ?? control.initial
          guard value.isFinite, control.range.contains(value) else {
            throw RuntimeError.message("Invalid ground trace control: \(control.key)")
          }
        }
      })
    definition.inspect = { clip, time, state, p in
      let event = studyEvent(p)
      let seconds = elapsed(clip, time, state, p)
      return ["elapsedMilliseconds": Float(seconds * 1_000),
        "sharedResponse": event.response(at: seconds), "sourceTriangles": shape == .boot ? 432 : 672,
        "footprintScale": recoveryPose(event: event, time: seconds, strength: 1).scale.x,
        "terrainDisplacement": 0]
    }
    return definition
  }
}
