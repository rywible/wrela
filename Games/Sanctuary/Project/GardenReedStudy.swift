import FieldCompiler
import FieldCore
import FieldEngine
import simd

/// Shared planted reed source, promoted after the matched 0306 isolated appearance gate.
/// Garden placement, saved growth and construction exclusion remain production-owned.
enum GardenReedStudy {
  static let name = "Garden reed cluster"
  static let rootAnchors: [V3] = [V3(-0.12, 0, 0.05), V3(0.09, 0, -0.08),
    V3(0.19, 0, 0.12), V3(-0.04, 0, -0.16)]
  static let stemHeights: [Float] = [0.82, 1.02, 0.69, 0.90]

  static func mesh(height: Float = 1.04, leafSpread: Float = 0.24,
    leafBend: Float = 0.10) -> Mesh {
    let heightScale = clamp(height, 0.75, 1.3) / 1.04
    let reach = clamp(leafSpread, 0.12, 0.32)
    let bend = clamp(leafBend, 0.04, 0.18)
    var result = Mesh()
    func vertex(_ point: V3, _ normal: V3, _ tint: V3) -> UInt32 {
      let index = UInt32(result.vertices.count)
      result.vertices.append(Vertex(point, normal, tint))
      return index
    }
    // Every ring winds around -tangent. Side faces and endpoint fans use the matching
    // outward winding. Closed components overlap at attachments, without detached cards.
    func rings(_ positions: [[V3]], centers: [V3], normals: [[V3]], color: V3) {
      let sides = positions[0].count
      var indices: [[UInt32]] = []
      for ring in positions.indices {
        var row: [UInt32] = []
        for side in 0..<sides { row.append(vertex(positions[ring][side], normals[ring][side], color)) }
        indices.append(row)
      }
      for ring in 0..<(indices.count - 1) {
        for side in 0..<sides {
          let next = (side + 1) % sides
          let a = indices[ring][side], b = indices[ring][next]
          let c = indices[ring + 1][side], d = indices[ring + 1][next]
          result.indices += [a, c, b, b, c, d]
        }
      }
      let startNormal = normalize(centers[0] - centers[1])
      let endNormal = normalize(centers[centers.count - 1] - centers[centers.count - 2])
      let first = vertex(centers[0], startNormal, color)
      let last = vertex(centers[centers.count - 1], endNormal, color)
      for side in 0..<sides {
        let next = (side + 1) % sides
        result.indices += [first, indices[0][side], indices[0][next]]
        result.indices += [last, indices[indices.count - 1][next], indices[indices.count - 1][side]]
      }
    }
    for stem in rootAnchors.indices {
      let anchor = rootAnchors[stem]
      let stemHeight = stemHeights[stem] * heightScale
      let angle = Float(stem) * 1.73 + 0.4
      let lean = V3(cos(angle), 0, sin(angle)) * (0.025 + Float(stem % 2) * 0.012)
      func center(_ t: Float) -> V3 { anchor + V3(0, stemHeight * t, 0) + lean * t * t }
      func tangent(_ t: Float) -> V3 { normalize(V3(0, stemHeight, 0) + lean * (2 * t)) }
      var stemPoints: [[V3]] = [], stemNormals: [[V3]] = [], stemCenters: [V3] = []
      for ring in 0...6 {
        let t = Float(ring) / 6
        let axis = tangent(t)
        let side = normalize(V3(1, 0, 0) - axis * axis.x)
        let up = cross(side, axis)
        let radius = (0.013 + Float(stem % 2) * 0.003) * (1 - 0.82 * t)
        var points: [V3] = [], normals: [V3] = []
        for segment in 0..<6 {
          let a = Float(segment) * 2 * Float.pi / 6
          let normal = side * cos(a) + up * sin(a)
          points.append(center(t) + normal * radius); normals.append(normal)
        }
        stemPoints.append(points); stemNormals.append(normals); stemCenters.append(center(t))
      }
      rings(stemPoints, centers: stemCenters, normals: stemNormals,
        color: V3(0.88 + Float(stem % 2) * 0.05, 0.96, 0.84))
      for leaf in 0..<2 {
        let attachment: Float = leaf == 0 ? 0.28 : 0.55
        let base = center(attachment)
        let azimuth = angle + Float(leaf) * 2.48
        let direction = V3(cos(azimuth), 0, sin(azimuth))
        let side = normalize(cross(direction, V3(0, 1, 0)))
        let length = reach * (leaf == 0 ? 1 : 0.79)
        let lift: Float = (leaf == 0 ? 0.10 : 0.09) * heightScale
        func leafCenter(_ t: Float) -> V3 {
          // A shallow continuous arc, not the steep roof-shaped silhouette observed
          // when the former sine arch was represented by only four straight spans.
          base + direction * (length * t)
            + V3(0, lift * t * (1 - t) - bend * 0.45 * t * t, 0)
        }
        func leafTangent(_ t: Float) -> V3 {
          normalize(direction * length + V3(0, lift * (1 - 2 * t) - bend * 0.9 * t, 0))
        }
        func leafPoint(_ t: Float, _ angle: Float) -> V3 {
          let up = normalize(cross(side, leafTangent(t)))
          // A smooth, nonzero width at the sealed ends also makes the differential
          // frame continuous there; the old max() profile introduced a small crease.
          let fullness: Float = 0.025 + 0.975 * sin(Float.pi * t)
          let width = (0.025 + Float(stem % 2) * 0.004) * fullness
          let thickness: Float = 0.004 * fullness
          return leafCenter(t) + side * (width * cos(angle)) + up * (thickness * sin(angle))
        }
        var leafPoints: [[V3]] = [], leafNormals: [[V3]] = [], leafCenters: [V3] = []
        for ring in 0...6 {
          let t = Float(ring) / 6
          var points: [V3] = [], normals: [V3] = []
          for edge in 0..<4 {
            let angle = Float(edge) * Float.pi / 2
            let along = leafPoint(min(1, t + 0.0005), angle) - leafPoint(max(0, t - 0.0005), angle)
            let around = leafPoint(t, angle + 0.0005) - leafPoint(t, angle - 0.0005)
            let normal = normalize(cross(along, around))
            points.append(leafPoint(t, angle)); normals.append(normal)
          }
          leafPoints.append(points); leafNormals.append(normals); leafCenters.append(leafCenter(t))
        }
        rings(leafPoints, centers: leafCenters, normals: leafNormals,
          color: V3(0.92 + Float(leaf) * 0.04, 1, 0.87))
      }
    }
    return result
  }

  static func batch(parameters: [String: Float] = [:]) -> SceneBatch {
    SceneBatch(name: name, mesh: mesh(height: parameters["height"] ?? 1.04,
      leafSpread: parameters["leafSpread"] ?? 0.24, leafBend: parameters["leafBend"] ?? 0.10),
      instances: [Instance(tint: V3(0.35, 0.48, 0.24), kind: 8)], roughness: 0.92)
  }

  /// Exact original four-capsule source and resolution for matched comparisons.
  static var baselineGenerator: AssetGenerator {
    let study = generator
    return AssetGenerator(id: "garden-reeds-baseline", name: "Garden · Reeds original",
      animation: study.animation, parts: study.parts,
      compile: { _ in [try LivingWorldPresentation.gardenReedBaselineBatch()] })
  }

  static var generator: AssetGenerator {
    let controls: [ScalarControl] = [.init("height", "Stem height · m", 1.04, 0.75...1.3),
      .init("leafSpread", "Leaf reach · m", 0.24, 0.12...0.32),
      .init("leafBend", "Leaf tip fall · m", 0.10, 0.04...0.18)]
    let animation = AnimationDefinition(controls: [], clips: ["grow", "settled"],
      scenarios: [], signalName: "Growth", duration: { _ in 1.35 },
      poses: { clip, time, _, _ in
        let elapsed: Double = clip == "settled" ? 1 : Double(max(0, time))
        let event = NatureMagicPresentation.event(patchID: 37, createdAtPlaySeconds: 0,
          playSeconds: elapsed)
        return [name: NatureMagicPresentation.growthPose(event: event, elementIndex: 0)]
      }, root: { _, _, _, _ in matrix_identity_float4x4 },
      initialize: { _ in BehaviorSnapshot() }, step: { state, _, _ in state },
      input: { _, _ in PreviewInput() },
      bounds: { _ in Bounds(V3(-0.8, -0.12, -0.8), V3(0.8, 1.6, 0.8)) },
      validate: { _ in })
    return AssetGenerator(id: "garden-reeds", name: "Garden · Reeds candidate",
      controls: controls, animation: animation,
      parts: { _ in [AssetPart(name: name, joint: PartJoint(id: name),
        field: .ellipsoid(V3(0, 0.5, 0), V3(0.5, 0.6, 0.5)),
        color: [0.35, 0.48, 0.24], material: 8)] },
      compile: { source in [batch(parameters: source.parameters)] })
  }
}
