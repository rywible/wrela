import FieldCompiler
import FieldCore
import FieldEngine
import SanctuaryContent
import simd

/// Sanctuary-owned botanical source. Curves, leaf blades and metre-valued controls are
/// authored here; the two compiled meshes are disposable, shared game/studio representations.
/// These are silhouette studies, not botanical growth or a branch-joint physics simulation.
enum BiomeVegetationDesign {
  static let grassRecipeIdentity = "sanctuary-cabin-grass-clumps-v1"
  static let regionalMeadowGrassRecipeIdentity = "sanctuary-regional-meadow-tuft-v1"

  /// One existing meadow root gets three leaves, without sampling another population anchor
  /// or advancing its RNG. This changes source geometry per plant, not plant spacing.
  static func meadowGrassTuft(_ source: GrassBlade) -> [GrassBlade] {
    var samples: [GrassBlade] = []
    samples.reserveCapacity(3)
    for leaf in 0..<3 {
      var sample = source
      sample.anchorAngle.w = source.anchorAngle.w + Float(leaf) * 1.37
      samples.append(sample)
    }
    return grassClump(samples)
  }

  /// Regroups the existing descriptor budget into three-leaf communities at existing
  /// root samples. Flower-bearing stems stay in place at their original height. No new
  /// RNG draws, population records, shaders or contact deformation are introduced.
  static func grassCommunity(
    _ sources: [GrassBlade], flowerStemIndices: Set<Int>
  ) -> (blades: [GrassBlade], clumpCount: Int) {
    var blades: [GrassBlade] = []
    blades.reserveCapacity(sources.count)
    var pending: [GrassBlade] = []
    pending.reserveCapacity(3)
    var clumps = 0
    for (index, source) in sources.enumerated() {
      if flowerStemIndices.contains(index) { blades.append(source); continue }
      pending.append(source)
      if pending.count == 3 {
        blades += grassClump(pending)
        clumps += 1
        pending.removeAll(keepingCapacity: true)
      }
    }
    // At most two remainder samples, still using the exact original source.
    blades += pending
    return (blades, clumps)
  }

  /// Three packed blades share one original contact root. Unequal heights and an open
  /// fan provide a short skirt instead of three unrelated vertical needles. The existing
  /// GrassBlade source supplies all five vertices, wind weights and contact response.
  private static func grassClump(_ sources: [GrassBlade]) -> [GrassBlade] {
    precondition(sources.count == 3)
    let root = sources[0].anchorAngle
    let height = sources[0].size.x
    let angles: [Float] = [-1.12, 0.18, 1.41]
    let heights: [Float] = [0.62, 0.84, 0.69]
    let widths: [Float] = [1.18, 1.12, 1.24]
    return sources.enumerated().map { index, source in
      var leaf = source
      // Reuse existing variation without advancing the source placement RNG.
      let variation = sin(source.anchorAngle.w * 1.7) * 0.12
      leaf.anchorAngle = SIMD4(root.x, root.y, root.z, root.w + angles[index] + variation)
      leaf.size = SIMD2(height * heights[index] + source.size.x * 0.06,
        source.size.y * widths[index])
      return leaf
    }
  }

  private static var grassGenerator: AssetGenerator {
    let heightRange: ClosedRange<Float> = 0.18...0.48
    let controls: [ScalarControl] = [
      ScalarControl("height", "Source blade height · m", Float(0.32), heightRange),
    ]
    let compile: (AssetSource) throws -> [SceneBatch] = { source in
      grassStudyBatches(source: source)
    }
    return AssetGenerator(id: "biome-grass-clumps", name: "Meadow · Grass communities",
      controls: controls, compile: compile)
  }

  private static func grassStudyBatches(source: AssetSource) -> [SceneBatch] {
    let height: Float = source.parameters["height"] ?? 0.32
    var blades: [GrassBlade] = []
    blades.reserveCapacity(21)
    for index in 0..<7 {
      let angle: Float = Float(index) * 2.399
      let radius: Float = index == 0 ? 0 : 0.48 + Float(index % 3) * 0.11
      let root = V3(cos(angle) * radius, 0, sin(angle) * radius)
      var sample: [GrassBlade] = []
      sample.reserveCapacity(3)
      for leaf in 0..<3 {
        let leafAngle: Float = angle + Float(leaf) * 1.37
        let heightVariation: Float = 0.84 + Float((index + leaf) % 4) * 0.065
        let leafHeight: Float = height * heightVariation
        let width: Float = 0.024 + Float((index * 2 + leaf) % 4) * 0.005
        let red: Float = 0.35 + Float(leaf) * 0.025
        let green: Float = 0.52 + Float(index % 3) * 0.02
        let color = V3(red, green, Float(0.23))
        let blade = GrassBlade(anchor: root, angle: leafAngle,
          height: leafHeight, width: width, color: color)
        sample.append(blade)
      }
      blades.append(contentsOf: grassClump(sample))
    }
    let instances: [Instance] = [Instance(kind: 3)]
    let batch = SceneBatch(name: "Grass communities", mesh: Mesh(), instances: instances,
      grass: blades, roughness: 0.9, doubleSided: true)
    return [batch]
  }

  enum Form: String, CaseIterable {
    case willow, palm, broadleaf

    var title: String {
      switch self {
      case .willow: return "Creek willow"
      case .palm: return "Rainforest palm"
      case .broadleaf: return "Meadow broadleaf"
      }
    }

    var height: Float { self == .palm ? 7 : 5.6 }
    var width: Float { self == .palm ? 6.2 : 6 }
    var trunk: Float { self == .palm ? 0.19 : 0.27 }
  }

  struct Design {
    let wood: Mesh
    let foliage: Mesh

    /// Wood and leaves must receive the same instance transform: their attachments,
    /// material-independent height-based shader wind and authoring scale then agree.
    func batches(name: String) -> [SceneBatch] {
      [
        SceneBatch(name: "\(name) wood", mesh: wood, instances: [Instance(kind: 4)],
          lodMeshes: [MeshProcessing.simplify(wood, ratio: 0.45, error: 0.035)],
          roughness: 0.9),
        SceneBatch(name: "\(name) foliage", mesh: foliage, instances: [Instance(kind: 8)],
          lodMeshes: [MeshProcessing.simplify(foliage, ratio: 0.5, error: 0.025)],
          roughness: 0.84, doubleSided: true),
      ]
    }
  }

  static var generators: [AssetGenerator] {
    Form.allCases.map { form in
      AssetGenerator(
        id: "biome-\(form.rawValue)", name: form.title,
        controls: [
          ScalarControl("height", "Growth height · m", form.height, 3...10),
          ScalarControl("width", "Canopy spread · m", form.width, 3...9),
          ScalarControl("trunk", "Trunk radius · m", form.trunk, 0.12...0.48),
          ScalarControl("droop", "Branch droop", 1, 0.65...1.25),
        ],
        compile: { source in make(form, parameters: source.parameters).batches(name: form.title) })
    } + [grassGenerator]
  }

  static func make(_ form: Form, parameters: [String: Float] = [:]) -> Design {
    let height = parameters["height"] ?? form.height
    let width = parameters["width"] ?? form.width
    let radius = parameters["trunk"] ?? form.trunk
    let droop = parameters["droop"] ?? 1
    switch form {
    case .willow: return willow(height: height, width: width, radius: radius, droop: droop)
    case .palm: return palm(height: height, width: width, radius: radius, droop: droop)
    case .broadleaf: return broadleaf(height: height, width: width, radius: radius, droop: droop)
    }
  }

  private static let bark = V3(0.40, 0.29, 0.20)

  private static func curve(_ a: V3, _ b: V3, _ c: V3, _ d: V3, _ t: Float) -> V3 {
    let s = 1 - t
    return a * (s * s * s) + b * (3 * s * s * t)
      + c * (3 * s * t * t) + d * (t * t * t)
  }

  /// Capped transported-frame tube. Branch bases overlap inside their parent tube, so
  /// no light gap opens at a joint. Overlap is deliberate; it is not a watertight union.
  private static func stem(
    _ path: (Float) -> V3, radius: (Float) -> Float,
    segments: Int = 8, sides: Int = 8, color: V3 = bark
  ) -> Mesh {
    var result = ParametricMesh.tube(
      segments: segments, sides: sides, center: path, radius: radius, color: color)
    for end in [0, segments] {
      let t = Float(end) / Float(segments)
      let normal = normalize(path(min(1, t + 0.001)) - path(max(0, t - 0.001)))
        * (end == 0 ? -1 : 1)
      let center = Vertex(path(t), normal, color)
      let ring = end * (sides + 1)
      for side in 0..<sides {
        var a = result.vertices[ring + side]
        var b = result.vertices[ring + side + 1]
        a.normal = SIMD4(normal, 0); b.normal = SIMD4(normal, 0)
        if end == 0 { result.triangle(center, b, a) }
        else { result.triangle(center, a, b) }
      }
    }
    return result
  }

  /// A pointed twelve-triangle blade with a folded central vein and a curved midrib.
  /// The explicit tip/root fans avoid collapsed parametric cells. No alpha cards or textures.
  private static func leaf(
    root: V3, bend: V3, tip: V3, side: V3, halfWidth: Float, color: V3
  ) -> Mesh {
    let across = normalize(side)
    let forward = normalize(tip - root)
    var normal = cross(forward, across)
    if length_squared(normal) < 0.00001 { normal = V3(0, 1, 0) }
    else { normal = normalize(normal) }
    var points = [root]
    for i in 1...3 {
      let t = Float(i) / 4
      let center = root * ((1 - t) * (1 - t)) + bend * (2 * t * (1 - t)) + tip * (t * t)
      let w = sin(t * .pi) * halfWidth
      points += [center - across * w, center + normal * (w * 0.22), center + across * w]
    }
    points.append(tip)
    var triangles = [(0, 1, 2), (0, 2, 3)]
    for ring in 0..<2 {
      let a = 1 + ring * 3, b = a + 3
      triangles += [(a, b, a + 1), (a + 1, b, b + 1),
        (a + 1, b + 1, a + 2), (a + 2, b + 1, b + 2)]
    }
    triangles += [(7, 10, 8), (8, 10, 9)]
    var result = Mesh()
    for (a, b, c) in triangles {
      let n = normalize(cross(points[b] - points[a], points[c] - points[a]))
      func vertex(_ index: Int) -> Vertex {
        let tint = color * (0.91 + 0.012 * Float(index))
        return Vertex(points[index], n, tint)
      }
      result.triangle(vertex(a), vertex(b), vertex(c))
    }
    return result
  }

  private static func roots(height: Float, radius: Float, hub: (Float) -> V3) -> [Mesh] {
    (0..<5).map { i in
      let angle = Float(i) * 2.399 + 0.2
      let direction = V3(cos(angle), 0, sin(angle))
      let reach = radius * (2.4 + 0.3 * Float(i % 3))
      return stem({ t in
        curve(hub(0.11), direction * (reach * 0.3) + V3(0, height * 0.03, 0),
          direction * (reach * 0.8), direction * reach + V3(0, 0.015, 0), t)
      }, radius: { radius * (0.5 * (1 - $0) + 0.035) }, segments: 5, sides: 7)
    }
  }

  private static func willow(height h: Float, width w: Float, radius r: Float, droop: Float) -> Design {
    let trunk = SanctuaryBotanicalTrunk(.willow, height: h, width: w, radius: r)
    let hub: (Float) -> V3 = { trunk.point(at: $0) }
    var wood = roots(height: h, radius: r, hub: hub)
    wood.append(stem(hub, radius: { trunk.radius(at: $0) }, segments: 16, sides: 12))
    var foliage: [Mesh] = []
    for branch in 0..<7 {
      let phase = Float(branch) * 1.73
      let angle = Float(branch) * 2.399 + 0.35 + 0.17 * sin(phase)
      let direction = V3(cos(angle), 0, sin(angle))
      let side = V3(-direction.z, 0, direction.x)
      let reach = w * (0.33 + 0.045 * sin(phase + 0.8))
      let start = hub(0.56 + 0.14 * (0.5 + 0.5 * sin(phase + 1.2)))
      let end = direction * reach + side * w * 0.035 * cos(phase)
        + V3(0.15, h * (0.78 + 0.065 * cos(phase + 0.3)), -0.1)
      let path: (Float) -> V3 = { t in
        curve(start, start + direction * (reach * 0.22) + V3(0, h * 0.29, 0),
          end - direction * (reach * 0.28) + V3(0, h * 0.1, 0), end, t)
      }
      wood.append(stem(path, radius: { r * (0.56 * (1 - $0) + 0.035) }, segments: 10))
      for spray in 0..<7 {
        let sprayPhase = phase + Float(spray) * 2.17
        let t = 0.29 + Float(spray) * 0.111 + 0.015 * sin(sprayPhase)
        let anchor = path(t)
        let fan = w * 0.12 * sin(sprayPhase + 0.7)
        let outward = direction * (w * (0.08 + 0.025 * cos(sprayPhase))) + side * fan
        let fall = h * (0.23 + 0.15 * (0.5 + 0.5 * sin(sprayPhase + 0.5))) * droop
        let tip = anchor + outward + V3(0, -fall, 0)
        let tassel: (Float) -> V3 = { u in
          curve(anchor, anchor + outward * 0.8 + V3(0, h * 0.07, 0),
            tip - outward * 0.2 + V3(0, fall * 0.42, 0), tip, u)
        }
        wood.append(stem(tassel, radius: { 0.022 * (1 - $0) + 0.003 }, segments: 8, sides: 5,
          color: bark * 1.05))
        for pair in 0..<12 {
          for sign in [Float(-1), 1] {
            let leafPhase = sprayPhase + Float(pair) * 1.41 + sign * 0.6
            let u = 0.025 + Float(pair) * 0.077 + (sign + 1) * 0.012
            let root = tassel(u)
            let length = h * (0.045 + 0.009 * sin(leafPhase))
            let leafSide = side * cos(leafPhase * 0.27) + direction * sin(leafPhase * 0.27)
            let lateral = leafSide * sign * length * 0.65 + direction * length * 0.14
            let end = root + lateral + V3(0, -length, 0)
            let shade = 0.94 + 0.06 * Float((pair + spray + branch) % 4)
            foliage.append(leaf(root: root, bend: root + lateral * 0.9 + V3(0, -length * 0.2, 0),
              tip: end, side: cross(leafSide, V3(0, 1, 0)), halfWidth: length * 0.22,
              color: V3(0.38, 0.51, 0.22) * shade))
          }
        }
      }
      // Short alternating leaves fill the scaffold, without another planar fern ladder.
      for pair in 0..<12 {
        for sign in [Float(-1), 1] {
          let root = path(0.13 + Float(pair) * 0.068 + (sign + 1) * 0.009)
          let leafPhase = phase + Float(pair) * 1.31
          let end = root + side * sign * w * (0.055 + 0.01 * sin(leafPhase))
            + direction * w * 0.025 + V3(0, h * 0.045 * sin(leafPhase), 0)
          foliage.append(leaf(root: root, bend: (root + end) * 0.5 + V3(0, h * 0.025, 0),
            tip: end, side: direction + V3(0, 0.35 * cos(leafPhase), 0),
            halfWidth: w * 0.014, color: V3(0.42, 0.54, 0.25)))
        }
      }
    }
    return Design(wood: ParametricMesh.joined(wood), foliage: ParametricMesh.joined(foliage))
  }

  private static func palm(height h: Float, width w: Float, radius r: Float, droop: Float) -> Design {
    let trunk = SanctuaryBotanicalTrunk(.palm, height: h, width: w, radius: r)
    let hub: (Float) -> V3 = { trunk.point(at: $0) }
    var wood: [Mesh] = [stem(hub, radius: { trunk.radius(at: $0) },
      segments: 30, sides: 12, color: V3(0.47, 0.36, 0.25))]
    var foliage: [Mesh] = []
    wood.append(stem({ t in hub(0.92 + t * 0.08) }, radius: { t in
      r * (0.9 + 0.32 * sin(t * .pi))
    }, segments: 6, sides: 12, color: V3(0.38, 0.41, 0.20)))
    for frond in 0..<13 {
      let angle = Float(frond) * 2.399 + 0.09 * sin(Float(frond) * 1.7)
      let direction = V3(cos(angle), 0, sin(angle))
      let side = V3(-direction.z, 0, direction.x)
      let tier = Float(frond % 3)
      let reach = w * (0.38 - tier * 0.047 + 0.014 * sin(Float(frond) * 1.4))
      let start = hub(0.96 + tier * 0.018)
      let end = start + direction * reach + V3(0, h * (0.02 + tier * 0.095 - 0.13 * droop), 0)
      let path: (Float) -> V3 = { t in
        curve(start, start + direction * (reach * 0.22) + V3(0, h * 0.27, 0),
          end - direction * (reach * 0.22) + V3(0, h * 0.18, 0), end, t)
      }
      wood.append(stem(path, radius: { 0.045 * (1 - $0) + 0.004 }, segments: 12, sides: 6,
        color: V3(0.37, 0.43, 0.18)))
      for pair in 0..<13 {
        let t = 0.12 + Float(pair) * 0.065
        let root = path(t)
        let length = w * (0.07 + 0.09 * pow(sin(t * .pi), 0.75)) * (1 - tier * 0.07)
        for sign in [Float(-1), 1] {
          let sweep = direction * length * 0.34 + side * sign * length
          let tip = root + sweep + V3(0, -length * (0.22 + t * 0.32) * droop, 0)
          let bend = root + sweep * 0.5 + V3(0, length * 0.11, 0)
          let tint = V3(0.28, 0.45, 0.20) * (0.95 + tier * 0.085 + Float(pair % 3) * 0.035)
          foliage.append(leaf(root: root, bend: bend, tip: tip, side: direction,
            halfWidth: length * (0.13 - tier * 0.005), color: tint))
        }
      }
      let tipRoot = path(0.9)
      foliage.append(leaf(root: tipRoot, bend: path(1), tip: end + direction * w * 0.023,
        side: side, halfWidth: w * 0.017, color: V3(0.34, 0.49, 0.21)))
    }
    return Design(wood: ParametricMesh.joined(wood), foliage: ParametricMesh.joined(foliage))
  }

  private static func broadleaf(height h: Float, width w: Float, radius r: Float, droop: Float) -> Design {
    let hub: (Float) -> V3 = { t in V3(0.16 * sin(t * 3), h * 0.76 * t, 0.18 * t * t) }
    var wood = roots(height: h, radius: r, hub: hub)
    wood.append(stem(hub, radius: { r * (1.05 - 0.86 * $0) }, segments: 16, sides: 12))
    var foliage: [Mesh] = []
    let tiers: [Float] = [0, 1, 0, 2, 1, 0, 2, 1, 0]
    for branch in 0..<9 {
      let phase = Float(branch) * 1.71
      let angle = Float(branch) * 2.399 + 0.6 + 0.13 * sin(phase)
      let direction = V3(cos(angle), 0, sin(angle))
      let side = V3(-direction.z, 0, direction.x)
      let tier = tiers[branch]
      let reach = w * (0.225 - tier * 0.070 + 0.015 * sin(phase))
      let start = hub(0.45 + tier * 0.17)
      let center = direction * reach + side * w * 0.025 * cos(phase)
        + V3(0, h * (0.62 + tier * 0.11 + 0.022 * sin(phase + 0.5)), 0.12)
      let extent = V3(w * (0.205 - tier * 0.017), h * (0.175 - tier * 0.016),
        w * (0.185 - tier * 0.013)) * (0.95 + 0.065 * cos(phase))
      let path: (Float) -> V3 = { t in
        curve(start, start + direction * reach * 0.25 + V3(0, h * 0.16, 0),
          center - direction * reach * 0.20 + V3(0, h * 0.07, 0), center, t)
      }
      wood.append(stem(path, radius: { r * (0.57 * (1 - $0) + 0.025) }, segments: 9))

      // A foliage aggregate establishes the rounded medium-scale mass. Its unequal,
      // gently lobed envelope overlaps adjacent groups; it is not one flattened cap.
      // The same source function anchors the small leaves, so none float off its surface.
      let envelope: (Float, Float) -> V3 = { azimuth, polar in
        let ring = sin(polar)
        let n = V3(ring * cos(azimuth), cos(polar), ring * sin(azimuth))
        let lobe = 1 + 0.09 * sin(azimuth * 3 + phase) * ring * ring
          + 0.055 * cos(polar * 5 + azimuth * 2 + phase) * ring
        var p = extent * n * lobe
        p.y -= extent.y * 0.12 * droop * ring * ring
        return center + p
      }
      let color = V3(0.35, 0.48, 0.21) * (0.94 + tier * 0.055 + 0.025 * sin(phase))
      var mass = ParametricMesh.surface(u: 24, v: 12, color: color) { u, v in
        envelope(u * 2 * .pi, (0.0001 + v * 0.9998) * .pi)
      }
      // Cap the tiny latitude rings explicitly, rather than using collapsed pole cells.
      for top in [true, false] {
        let normal = V3(0, top ? 1 : -1, 0)
        let pole = Vertex(center + normal * extent.y, normal, color)
        let base = top ? 0 : 12 * 25
        for segment in 0..<24 {
          let a = mass.vertices[base + segment], b = mass.vertices[base + segment + 1]
          if top { mass.triangle(pole, b, a) }
          else { mass.triangle(pole, a, b) }
        }
      }
      foliage.append(mass)
      for index in 0..<60 {
        let y = 1 - 2 * (Float(index) + 0.5) / 60
        let polar = acos(y)
        let azimuth = Float(index) * 2.399 + phase
        let n = V3(sin(polar) * cos(azimuth), y, sin(polar) * sin(azimuth))
        let across = normalize(cross(abs(y) < 0.94 ? V3(0, 1, 0) : V3(1, 0, 0), n))
        let along = cross(n, across)
        let twist = Float(index) * 1.37 + phase
        let bladeDirection = along * cos(twist) + across * sin(twist)
        let bladeSide = cross(n, bladeDirection)
        let root = envelope(azimuth, polar)
        let blade = w * (0.043 + 0.007 * sin(Float(index) * 1.17 + phase))
        let tip = root + bladeDirection * blade + n * blade * 0.20
          + V3(0, -blade * 0.10 * droop, 0)
        foliage.append(leaf(root: root,
          bend: root + bladeDirection * blade * 0.48 + n * blade * 0.30,
          tip: tip, side: bladeSide, halfWidth: blade * 0.37,
          color: color * (0.97 + 0.07 * sin(Float(index) * 0.83))))
      }
    }
    return Design(wood: ParametricMesh.joined(wood), foliage: ParametricMesh.joined(foliage))
  }
}
