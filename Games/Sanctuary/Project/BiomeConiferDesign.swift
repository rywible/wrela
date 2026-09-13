import FieldCompiler
import FieldCore
import FieldEngine
import SanctuaryContent
import simd

/// An authored alpine fir: curved leader, unequal bough whorls and grouped needle sprays.
/// The sprays describe groups of needles at play distance, rather than individual needles.
/// The isolated candidate is provisional for regional scene review, not an approved baseline.
enum BiomeConiferDesign {
  struct Design {
    let wood: Mesh
    let foliage: Mesh

    func batches(name: String = "Alpine fir") -> [SceneBatch] {
      [
        SceneBatch(name: "\(name) wood", mesh: wood, instances: [Instance(kind: 4)],
          lodMeshes: [MeshProcessing.simplify(wood, ratio: 0.45, error: 0.035)], roughness: 0.9),
        SceneBatch(name: "\(name) needles", mesh: foliage, instances: [Instance(kind: 8)],
          lodMeshes: [MeshProcessing.simplify(foliage, ratio: 0.5, error: 0.025)],
          roughness: 0.87, doubleSided: true),
      ]
    }
  }

  /// Studio and regional consumers reuse these immutable source meshes and LODs once per process.
  /// Every batch must receive the same uniform placement transform and neutral instance tint.
  static let defaultBatches = make().batches()

  static var generator: AssetGenerator {
    AssetGenerator(id: "biome-conifer", name: "Alpine fir", controls: [
      ScalarControl("height", "Growth height · m", 8, 4...11),
      ScalarControl("width", "Bough spread · m", 5.2, 3...7),
      ScalarControl("trunk", "Trunk radius · m", 0.24, 0.14...0.4),
      ScalarControl("droop", "Bough droop", 1, 0.65...1.3),
    ], compile: { source in
      if source.parameters.isEmpty { return defaultBatches }
      return make(parameters: source.parameters).batches()
    })
  }

  static func make(parameters: [String: Float] = [:]) -> Design {
    let height = parameters["height"] ?? 8
    let width = parameters["width"] ?? 5.2
    let radius = parameters["trunk"] ?? 0.24
    let droop = parameters["droop"] ?? 1
    let trunk = SanctuaryBotanicalTrunk(.conifer, height: height, width: width, radius: radius)
    func leader(_ t: Float) -> V3 {
      trunk.point(at: t)
    }
    var wood = [tube(leader, radius: { trunk.radius(at: $0) },
      segments: 22, sides: 10)]
    var foliage: [Mesh] = []
    // Small root flares anchor the tree without making a circular pedestal.
    for index in 0..<4 {
      let angle = Float(index) * 2.399
      let direction = V3(cos(angle), 0, sin(angle))
      wood.append(tube({ t in
        leader(0.065 * (1 - t)) + direction * (radius * 2.5 * t)
      }, radius: { radius * (0.52 - 0.46 * $0) }, segments: 5, sides: 7))
    }

    let levels: [Float] = [0.23, 0.305, 0.405, 0.49, 0.585, 0.68, 0.755, 0.825, 0.895, 0.952]
    let spreads: [Float] = [1, 0.92, 0.84, 0.74, 0.62, 0.50, 0.40, 0.31, 0.22, 0.12]
    let counts = [6, 6, 5, 5, 5, 4, 4, 3, 3, 2]
    for tier in levels.indices {
      let count = counts[tier]
      for branch in 0..<count {
        let phase = Float(branch) * 2.17 + Float(tier) * 1.31
        let angle = Float(branch) * 2 * .pi / Float(count) + Float(tier) * 1.19
          + 0.11 * sin(phase)
        let direction = V3(cos(angle), 0, sin(angle))
        let across = V3(-sin(angle), 0, cos(angle))
        let reach = width * 0.5 * spreads[tier] * (0.91 + 0.09 * sin(phase + 0.8))
        let level = levels[tier] + (0.031 - Float(tier) * 0.0025) * sin(phase * 1.3)
        let start = leader(level)
        let drop = height * 0.063 * droop * (0.76 - level * 0.4)
        let rising = min(height * 0.055 * level * level, height * (1 - level) * 0.6)
        func path(_ t: Float) -> V3 {
          start + direction * (reach * t) + across * (reach * 0.035 * sin(t * .pi + phase) * t)
            + V3(0, -drop * sin(t * .pi * 0.82) + height * 0.023 * t * t + rising * t, 0)
        }
        let branchRadius = radius * (0.32 - Float(tier) * 0.025)
        wood.append(tube(path, radius: { branchRadius * (1 - 0.92 * $0) },
          segments: 6, sides: 6))
        let tint = V3(0.24, 0.38, 0.29) * (0.96 + Float(tier) * 0.014 + 0.035 * sin(phase))
        // A narrow rounded core joins overlapping groups. The old broad, flattened core
        // and lateral oval plates read as fern leaves; every exposed tip now belongs to
        // a slender needle group, distributed around the bough in three dimensions.
        foliage.append(spray({ t in path(0.04 + 0.98 * t) },
          halfWidth: reach * 0.057, thickness: reach * 0.052,
          segments: 6, sides: 6, color: tint))
        for pair in 0..<4 {
          let t = 0.10 + Float(pair) * 0.205
          for sign: Float in [-1, 1] {
            let base = path(t + sign * 0.023) + V3(0, reach * 0.022, 0)
            let groupLength = reach * (0.43 - Float(pair) * 0.055)
            for needle in 0..<3 {
              let lift = Float(needle) * 0.42 - 0.20
              let fan = 0.27 + Float(needle) * 0.13
              let needleDirection = normalize(direction * 0.78 + across * (sign * fan)
                + V3(0, lift, 0))
              let length = groupLength * (0.88 + 0.10 * Float(needle))
              let root = base + across * (sign * reach * 0.016 * Float(needle))
              foliage.append(spray({ u in
                root + needleDirection * (length * u)
                  + V3(0, length * 0.09 * sin(u * .pi), 0)
              }, halfWidth: length * 0.10, thickness: length * 0.09,
                segments: 3, sides: 4, color: tint * (0.98 + Float(needle) * 0.025)))
            }
          }
        }
      }
    }
    // Small overlapping leader shoots continue the crown taper. There is no separate
    // oversized bud or leaf tuft above the last branch tier.
    for group in 0..<6 {
      let t = 0.84 + Float(group) * 0.027
      let spread = width * (0.089 - Float(group) * 0.013)
      for needle in 0..<5 {
        let angle = Float(needle) * 2.399 + Float(group) * 1.37
        let root = leader(t)
        let tip = leader(min(1, t + 0.056)) + V3(cos(angle), 0, sin(angle)) * spread
        foliage.append(spray({ u in root * (1 - u) + tip * u },
          halfWidth: spread * 0.12, thickness: spread * 0.10,
          segments: 3, sides: 4, color: V3(0.27, 0.41, 0.30)))
      }
    }
    return Design(wood: ParametricMesh.joined(wood), foliage: ParametricMesh.joined(foliage))
  }

  private static func tube(
    _ path: (Float) -> V3, radius: (Float) -> Float, segments: Int, sides: Int
  ) -> Mesh {
    let color = V3(0.40, 0.29, 0.21)
    var result = ParametricMesh.tube(segments: segments, sides: sides,
      center: path, radius: radius, color: color)
    for end in [0, segments] {
      let t = Float(end) / Float(segments)
      let normal = normalize(path(min(1, t + 0.001)) - path(max(0, t - 0.001)))
        * (end == 0 ? -1 : 1)
      let center = Vertex(path(t), normal, color)
      for side in 0..<sides {
        var a = result.vertices[end * (sides + 1) + side]
        var b = result.vertices[end * (sides + 1) + side + 1]
        a.normal = SIMD4(normal, 0); b.normal = SIMD4(normal, 0)
        if end == 0 { result.triangle(center, b, a) }
        else { result.triangle(center, a, b) }
      }
    }
    return result
  }

  /// Closed tapered needle cluster with a softly ridged upper surface. Explicit tip fans
  /// avoid collapsed end rings; smooth normals keep the broad mass quiet under moving light.
  private static func spray(
    _ path: (Float) -> V3, halfWidth: Float, thickness: Float,
    segments: Int, sides: Int, color: V3
  ) -> Mesh {
    var mesh = Mesh()
    var rings: [[Vertex]] = []
    for row in 1..<segments {
      let t = Float(row) / Float(segments)
      let tangent = normalize(path(min(1, t + 0.001)) - path(max(0, t - 0.001)))
      let reference = abs(tangent.y) > 0.92 ? V3(1, 0, 0) : V3(0, 1, 0)
      let across = normalize(cross(tangent, reference))
      let up = cross(across, tangent)
      let envelope = pow(sin(t * .pi), 0.78) * (1.14 - 0.28 * t)
      var ring: [Vertex] = []
      for side in 0..<sides {
        let angle = Float(side) * 2 * .pi / Float(sides)
        let lateral = across * (cos(angle) * halfWidth)
        let vertical = up * (sin(angle) * thickness)
        let point = path(t) + (lateral + vertical) * envelope
        let normal = normalize(across * (cos(angle) / halfWidth)
          + up * (sin(angle) / thickness) + tangent * (0.35 * (t - 0.5)))
        let shade = 0.92 + 0.08 * max(0, sin(angle)) + 0.04 * t
        ring.append(Vertex(point, normal, color * shade))
      }
      rings.append(ring)
    }
    for row in 0..<(rings.count - 1) {
      for side in 0..<sides {
        let next = (side + 1) % sides
        mesh.triangle(rings[row][side], rings[row + 1][side], rings[row][next])
        mesh.triangle(rings[row][next], rings[row + 1][side], rings[row + 1][next])
      }
    }
    let startNormal = normalize(path(0) - path(0.001))
    let endNormal = normalize(path(1) - path(0.999))
    let start = Vertex(path(0), startNormal, color * 0.92)
    let end = Vertex(path(1), endNormal, color * 1.04)
    for side in 0..<sides {
      let next = (side + 1) % sides
      mesh.triangle(start, rings[0][side], rings[0][next])
      mesh.triangle(end, rings[rings.count - 1][next], rings[rings.count - 1][side])
    }
    return mesh
  }
}
