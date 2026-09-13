import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import simd

/// Shared source-space mount anatomy. All pivots stay fixed when surface controls change.
/// Body envelopes are continuous monotone section lofts; compiled meshes are representations.
enum SanctuaryMountDesign {
  typealias Part = LivingWorldPresentation.PartDesign
  static func controls(for species: WildlifeSpecies) -> [ScalarControl] {
    switch species {
    case .moonhart:
      return [ScalarControl("bodyFullness", "Chest and flank fullness", 1, 0.88...1.08),
        ScalarControl("muzzleTaper", "Muzzle width", 1, 0.86...1.1),
        ScalarControl("earLength", "Listening ear length", 1, 0.85...1.12),
        ScalarControl("eyeSize", "Eye aperture", 1, 0.88...1.1),
        ScalarControl("antlerSpread", "Antler crown spread", 1, 0.88...1.12)]
    case .cloudRay:
      return [ScalarControl("wingSweep", "Wing sweep", 0.32, 0.18...0.44),
        ScalarControl("mantleDepth", "Mantle depth", 1, 0.88...1.12),
        ScalarControl("eyeSize", "Eye aperture", 1, 0.88...1.1)]
    default: return []
    }
  }

  private struct RecipeKey: Hashable {
    let species: String
    let values: [UInt32]
  }
  /// Pose/metadata queries share immutable mesh storage with the source recipe. Six recent
  /// parameter variants bound memory; eviction cannot invalidate an already returned array.
  private final class Recipes: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [RecipeKey: [Part]] = [:]
    private var recent: [RecipeKey] = []

    func get(_ species: WildlifeSpecies, parameters: [String: Float]) -> [Part] {
      let controls = SanctuaryMountDesign.controls(for: species)
      guard !controls.isEmpty else { return [] }
      let resolved = controls.map { parameters[$0.key] ?? $0.initial }
      let key = RecipeKey(species: species.rawValue, values: resolved.map(\.bitPattern))
      lock.lock(); defer { lock.unlock() }
      if let index = recent.firstIndex(of: key) { recent.remove(at: index) }
      recent.append(key)
      if let cached = values[key] { return cached }
      let effective = Dictionary(uniqueKeysWithValues: zip(controls.map(\.key), resolved))
      let result = species == .moonhart ? moonhart(effective) : cloudRay(effective)
      if values.count >= 6 { values.removeValue(forKey: recent.removeFirst()) }
      values[key] = result
      return result
    }
  }
  private static let recipes = Recipes()

  static func parts(for species: WildlifeSpecies, parameters: [String: Float] = [:]) -> [Part] {
    recipes.get(species, parameters: parameters)
  }

  struct Leg {
    let id: Int
    let hip: V3
    let knee: V3
    let foot: V3
    var upperName: String { "Wildlife moonhart upper leg \(id)" }
    var lowerName: String { "Wildlife moonhart lower leg \(id)" }
    var hoofName: String { "Wildlife moonhart hoof \(id)" }
  }
  static let legs: [Leg] = (0..<4).map { id in
    let side: Float = id % 2 == 0 ? -1 : 1
    let front = id < 2
    return Leg(id: id, hip: V3(side * 0.255, front ? 1.06 : 1.08, front ? -0.40 : 0.56),
      knee: V3(side * 0.255, front ? 0.50 : 0.47, front ? -0.35 : 0.70),
      foot: V3(side * 0.255, 0.095, front ? -0.44 : 0.51))
  }
  static let headPivot = V3(0, 1.42, -0.52)

  private static func moonhart(_ p: [String: Float]) -> [Part] {
    let fullness = p["bodyFullness"] ?? 1, muzzle = p["muzzleTaper"] ?? 1
    let ear = p["earLength"] ?? 1, eye = p["eyeSize"] ?? 1, crown = p["antlerSpread"] ?? 1
    let bodyName = "Wildlife moonhart body", headName = "Wildlife moonhart head"
    let coat = V3(0.53, 0.58, 0.59), cream = V3(0.78, 0.77, 0.68)
    func section(_ id: String, _ z: Float, _ y: Float, _ x: Float, _ h: Float) -> LoftSection {
      .init(id: id, z: z, center: SIMD2(0, y), radius: SIMD2(x, h))
    }
    let body = SectionLoft([
      section("throat-tip", -0.80, 1.39, 0.012, 0.014),
      section("throat", -0.65, 1.36, 0.16, 0.28),
      section("chest", -0.47, 1.17, 0.26 * fullness, 0.40),
      section("shoulder", -0.25, 1.07, 0.33 * fullness, 0.35),
      section("saddle", 0.10, 1.06, 0.33 * fullness, 0.29),
      section("haunch", 0.51, 1.06, 0.32 * fullness, 0.30),
      section("rump", 0.76, 1.04, 0.22, 0.23),
      section("rump-tip", 0.92, 1.05, 0.012, 0.014),
    ])
    let head = SectionLoft([
      section("nose-tip", -1.145, 1.49, 0.012, 0.014),
      section("nose", -1.10, 1.49, 0.10 * muzzle, 0.075),
      section("muzzle", -0.98, 1.51, 0.13 * muzzle, 0.105),
      section("cheek", -0.82, 1.57, 0.19, 0.18),
      section("brow", -0.63, 1.65, 0.235, 0.225),
      section("poll", -0.46, 1.66, 0.19, 0.19),
      section("poll-tip", -0.33, 1.63, 0.012, 0.014),
    ])
    func part(_ suffix: String, _ shape: Shape, _ color: V3, mesh: Mesh? = nil,
      role: LivingWorldPresentation.CreatureRole = .body, pivot: V3 = .zero,
      parent: String? = nil, rough: Float = 0.85) -> Part {
      Part(name: "Wildlife moonhart \(suffix)", shape: shape, color: color, material: 7,
        roughness: rough, role: role, pivot: pivot, proceduralMesh: mesh, parent: parent, resolution: 36)
    }
    var result = [
      part("body", .loft(body), V3(repeating: 1), mesh: loftMesh(body, rings: 48, sides: 32) { point in
        let underside = 1 - smooth(0.81, 1.03, point.y)
        return mix(coat, cream, underside * 0.48)
      }),
      part("head", .loft(head), V3(repeating: 1), mesh: loftMesh(head, rings: 40, sides: 32) { point in
        let jaw = (1 - smooth(1.48, 1.58, point.y)) * (1 - smooth(-0.87, -0.66, point.z))
        return mix(coat + V3(repeating: 0.035), cream, jaw * 0.82)
      }, role: .head, pivot: headPivot, parent: bodyName),
    ]
    // Small dark nose follows the long upper muzzle; the chin is a color transition in the head.
    let noseCenter = V3(0, 1.505, -1.134)
    result.append(part("nose", ellipsoid(noseCenter, V3(0.060, 0.038, 0.024)),
      V3(0.17, 0.23, 0.23), mesh: ParametricMesh.ellipsoid(noseCenter, V3(0.060, 0.038, 0.024), detail: 12),
      parent: headName, rough: 0.48))
    let mouth = ParametricMesh.tube(segments: 16, sides: 6, center: { t in
      let x = (t - 0.5) * 0.155
      return V3(x, 1.447 + 0.018 * pow(abs(t - 0.5) * 2, 2), -1.061 + 0.10 * x * x)
    }, radius: { _ in 0.006 })
    result.append(part("mouth", .box(V3(0.09, 0.022, 0.012)).moved(V3(0, 1.456, -1.061)),
      V3(0.30, 0.34, 0.32), mesh: mouth, parent: headName))
    for side: Float in [-1, 1] {
      let suffix = "\(side)"
      let start = V3(0, 1.69, -0.65), direction = normalize(V3(side * 0.87, 0.08, -0.49))
      let seated = surfaceSeat(.loft(head), from: start, direction: direction)
      let q = simd_quatf(from: V3(0, 0, -1), to: seated.normal)
      let eyeName = "Wildlife moonhart eye \(suffix)"
      let aperture = V3(0.047, 0.060, 0.014) * eye
      let eyeShape = ellipsoid(.zero, aperture).rotated(q).moved(seated.point + seated.normal * 0.002)
      result.append(part("eye \(suffix)", eyeShape, V3(0.35, 0.41, 0.39),
        mesh: rotatedEllipsoid(seated.point + seated.normal * 0.002, aperture, q, detail: 12),
        role: .eyes, pivot: seated.point, parent: headName, rough: 0.50))
      let iris = seated.point + seated.normal * 0.023
      let irisRadius = V3(0.041, 0.054, 0.013) * eye
      result.append(part("iris \(suffix)", ellipsoid(.zero, irisRadius).rotated(q).moved(iris),
        V3(0.025, 0.047, 0.048), mesh: rotatedEllipsoid(iris, irisRadius, q, detail: 12),
        parent: eyeName, rough: 0.36))
      let glint = iris + q.act(V3(-0.010, 0.019, -0.012) * eye)
      result.append(part("eye glint \(suffix)", ellipsoid(glint, V3(repeating: 0.006 * eye)),
        V3(0.81, 0.84, 0.77), mesh: ParametricMesh.ellipsoid(glint, V3(repeating: 0.006 * eye), detail: 6),
        parent: eyeName, rough: 0.42))
      let earPivot = V3(side * 0.155, 1.78, -0.48)
      let earCenter = earPivot + V3(side * 0.115, 0.12 * ear, 0.015)
      let earRotation = simd_quatf(angle: -side * 0.91, axis: V3(0, 0, 1))
      let earRadius = V3(0.072, 0.205 * ear, 0.060)
      let earName = "Wildlife moonhart ear \(suffix)"
      result.append(part("ear \(suffix)", ellipsoid(.zero, earRadius).rotated(earRotation).moved(earCenter), coat,
        mesh: rotatedEllipsoid(earCenter, earRadius, earRotation, detail: 14),
        role: .ear(side), pivot: earPivot, parent: headName))
      let lining = earCenter + V3(0, 0, -0.041)
      result.append(part("ear lining \(suffix)", ellipsoid(.zero, earRadius * V3(0.66, 0.73, 0.34)).rotated(earRotation).moved(lining),
        V3(0.62, 0.53, 0.49), mesh: rotatedEllipsoid(lining, earRadius * V3(0.66, 0.73, 0.34), earRotation, detail: 12),
        parent: earName))
      func a(_ x: Float, _ y: Float, _ z: Float) -> V3 { V3(side * x * crown, y, z) }
      let stem = [a(0.12, 1.80, -0.47), a(0.16, 2.02, -0.36), a(0.27, 2.21, -0.20), a(0.40, 2.34, -0.06)]
      let antler = ParametricMesh.joined([
        curvedTube(stem, radius: 0.033, tip: 0.005, segments: 28, sides: 9),
        curvedTube([stem[1], a(0.31, 2.09, -0.47), a(0.42, 2.16, -0.51)], radius: 0.023, tip: 0.003, segments: 16, sides: 8),
        curvedTube([stem[2], a(0.39, 2.29, -0.26), a(0.48, 2.38, -0.29)], radius: 0.018, tip: 0.003, segments: 16, sides: 8),
      ])
      result.append(part("moon antler \(suffix)", .box(V3(0.23, 0.31, 0.27)).moved(a(0.28, 2.09, -0.28)),
        V3(0.76, 0.73, 0.60), mesh: antler, parent: headName, rough: 0.73))
    }
    let tail = curvedTube([V3(0, 1.13, 0.74), V3(0, 1.20, 0.91), V3(0, 1.25, 1.04)],
      radius: 0.095, tip: 0.013, segments: 18, sides: 12)
    result.append(part("tail", .box(V3(0.10, 0.12, 0.17)).moved(V3(0, 1.18, 0.90)), cream,
      mesh: tail, role: .expressive, pivot: V3(0, 1.13, 0.74), parent: bodyName))
    for leg in legs {
      let front = leg.id < 2
      let mid = front ? mix(leg.hip, leg.knee, 0.48) : V3(leg.hip.x, 0.72, 0.40)
      // Begin inside the torso, rather than exposing the cut ring outside its flank.
      let buriedRoot = V3(leg.hip.x * 0.68, leg.hip.y + 0.09, leg.hip.z)
      let upper = ParametricMesh.joined([
        curvedTube([buriedRoot, leg.hip, mid, leg.knee], radius: front ? 0.085 : 0.095,
          tip: 0.052, segments: 24, sides: 12),
        ParametricMesh.ellipsoid(leg.knee, V3(0.056, 0.060, 0.056), detail: 10),
      ])
      result.append(part("upper leg \(leg.id)", .box(V3(0.20, 0.46, 0.26)).moved((leg.hip + leg.knee) * 0.5 + V3(0, 0.04, 0)), coat,
        mesh: upper, role: .leg(leg.id), pivot: leg.hip, parent: bodyName))
      let lower = curvedTube([leg.knee, mix(leg.knee, leg.foot, 0.65), leg.foot], radius: 0.054,
        tip: 0.042, segments: 16, sides: 10)
      result.append(part("lower leg \(leg.id)", .capsule(leg.knee, leg.foot, 0.056), coat * 0.90,
        mesh: lower, pivot: leg.knee, parent: leg.upperName))
      let hoofCenter = V3(leg.foot.x, 0.052, leg.foot.z - 0.025)
      let hoof = ParametricMesh.joined([-1, 1].map { sign in
        ParametricMesh.ellipsoid(hoofCenter + V3(Float(sign) * 0.032, 0, 0), V3(0.039, 0.052, 0.095), detail: 10)
      })
      result.append(part("hoof \(leg.id)", .box(V3(0.075, 0.052, 0.10)).moved(hoofCenter),
        V3(0.26, 0.30, 0.29), mesh: hoof, pivot: leg.foot, parent: leg.lowerName))
    }
    return result
  }

  private static func cloudRay(_ p: [String: Float]) -> [Part] {
    let sweep = p["wingSweep"] ?? 0.32, depth = p["mantleDepth"] ?? 1, eye = p["eyeSize"] ?? 1
    let bodyName = "Wildlife cloudRay head"
    let point: (Float, Float) -> V3 = { u, v in rayPoint(u, v, sweep: sweep, depth: depth) }
    let mesh = ParametricMesh.surface(u: 112, v: 44, position: point, tint: { u, v in
      let upper = smooth(-0.18, 0.22, cos(v * .pi))
      let span = abs(point(u, v).x) / 2.1
      let dorsal = mix(V3(0.32, 0.52, 0.60), V3(0.45, 0.64, 0.68), smooth(0.2, 0.92, span) * 0.65)
      return mix(V3(0.71, 0.76, 0.68), dorsal, upper)
    })
    var body = Part(name: bodyName, shape: .box(V3(2.12, 0.43, 1.36)),
      color: V3(repeating: 1), material: 7, roughness: 0.78, role: .body,
      cloudRaySurface: true, proceduralMesh: mesh)
    body.pivot = .zero
    var result = [body]
    for side: Float in [-1, 1] {
      let u: Float = 0.75 + side * 0.030, v: Float = 0.32
      let center = point(u, v)
      let offset = ParametricMesh.offsetPoint(u: u, v: v, distance: 0.01, position: point)
      let normal = normalize(offset - center), q = simd_quatf(from: V3(0, 0, -1), to: normal)
      let eyeCenter = center + normal * 0.002
      let eyeName = "Wildlife cloudRay eye \(side)"
      let aperture = V3(0.068, 0.048, 0.014) * eye
      result.append(Part(name: eyeName, shape: ellipsoid(.zero, aperture).rotated(q).moved(eyeCenter),
        color: V3(0.43, 0.59, 0.60), material: 7, roughness: 0.49, role: .eyes,
        pivot: center, proceduralMesh: rotatedEllipsoid(eyeCenter, aperture, q, detail: 12), parent: bodyName))
      let iris = center + normal * 0.026
      let irisRadius = V3(0.058, 0.040, 0.014) * eye
      result.append(Part(name: "Wildlife cloudRay iris \(side)", shape: ellipsoid(.zero, irisRadius).rotated(q).moved(iris),
        color: V3(0.021, 0.048, 0.063), material: 7, roughness: 0.36,
        proceduralMesh: rotatedEllipsoid(iris, irisRadius, q, detail: 12), parent: eyeName))
      let glint = iris + q.act(V3(-0.017, 0.011, -0.014) * eye)
      result.append(Part(name: "Wildlife cloudRay eye glint \(side)",
        shape: ellipsoid(glint, V3(repeating: 0.007 * eye)),
        color: V3(0.79, 0.84, 0.81), material: 7, roughness: 0.42,
        proceduralMesh: ParametricMesh.ellipsoid(glint, V3(repeating: 0.007 * eye), detail: 6), parent: eyeName))
    }
    // A single anatomical tail overlaps the mantle's narrow posterior root and tapers continuously.
    let tail = curvedTube([V3(0, -0.02, 0.74), V3(0, -0.035, 1.30), V3(0, 0.12, 2.20), V3(0, 0.23, 2.80)],
      radius: 0.095, tip: 0.006, segments: 48, sides: 12)
    result.append(Part(name: "Wildlife cloudRay trailing tail", shape: .box(V3(0.13, 0.24, 1.08)).moved(V3(0, 0.08, 1.77)),
      color: V3(0.34, 0.53, 0.60), material: 7, roughness: 0.80, role: .expressive,
      pivot: V3(0, 0.07, 0.92), proceduralMesh: tail, parent: bodyName))
    return result
  }

  private static func rayPoint(_ u: Float, _ v: Float, sweep: Float, depth: Float) -> V3 {
    let angle = u * (2 * Float.pi), latitude = (0.0001 + v * 0.9998) * Float.pi
    let radial = sin(latitude), x = 2.1 * radial * cos(angle), span = abs(x) / 2.1
    // Collapsing the outer chord makes swept manta wings instead of an oval dinner plate.
    let z = 1.22 * radial * sin(angle) * (1 - 0.58 * pow(span, 0.8)) + sweep * span * span
    let lobe = exp(-pow((abs(x) - 0.28) / 0.14, 2)) * (1 - smooth(-0.95, -0.65, z))
    let thickness = (0.09 + 0.21 * exp(-span * span * 11)) * depth
    let y = cos(latitude) * thickness + 0.16 * span * span - 0.032 * z
    return V3(x, y, z - 0.09 * lobe)
  }

  private static func surfaceSeat(_ shape: Shape, from origin: V3, direction: V3) -> (point: V3, normal: V3) {
    var a: Float = 0, b: Float = 0.6
    for _ in 0..<30 {
      let middle = (a + b) * 0.5
      if shape.value(at: origin + direction * middle) > 0 { b = middle } else { a = middle }
    }
    let point = origin + direction * ((a + b) * 0.5)
    return (point, normalize(shape.sample(at: point).gradient))
  }

  private static func ellipsoid(_ center: V3, _ radius: V3) -> Shape { Shape.sphere(1).stretched(radius).moved(center) }
  private static func rotatedEllipsoid(_ center: V3, _ radius: V3, _ q: simd_quatf, detail: Int) -> Mesh {
    ParametricMesh.surface(u: detail * 2, v: detail) { u, v in
      let a = u * 2 * Float.pi, b = (0.0001 + v * 0.9998) * Float.pi
      return center + q.act(radius * V3(sin(b) * cos(a), cos(b), sin(b) * sin(a)))
    }
  }
  private static func loftMesh(_ loft: SectionLoft, rings: Int, sides: Int, color: @escaping (V3) -> V3) -> Mesh {
    let lo = loft.sections.first!.z, hi = loft.sections.last!.z
    func point(_ u: Float, _ v: Float) -> V3 {
      let z = lo + (hi - lo) * v, section = loft.profile(at: z).value, a = u * 2 * Float.pi
      return V3(section.x + cos(a) * section.z, section.y + sin(a) * section.w, z)
    }
    var mesh = ParametricMesh.surface(u: sides, v: rings, position: point, tint: { u, v in color(point(u, v)) })
    for end in [0, rings] {
      let z = end == 0 ? lo : hi, section = loft.profile(at: z).value
      let center = UInt32(mesh.vertices.count), p = V3(section.x, section.y, z)
      mesh.vertices.append(Vertex(p, V3(0, 0, end == 0 ? -1 : 1), color(p)))
      for i in 0..<sides {
        let a = UInt32(end * (sides + 1) + i), b = a + 1
        mesh.indices += end == 0 ? [center, b, a] : [center, a, b]
      }
    }
    return mesh
  }
  private static func curvedTube(_ points: [V3], radius: Float, tip: Float, segments: Int, sides: Int) -> Mesh {
    var mesh = ParametricMesh.tube(segments: segments, sides: sides, center: { t in
      // A quadratic/cubic Bezier is one continuous authored bone/tail/antler envelope.
      var row = points
      while row.count > 1 { row = zip(row, row.dropFirst()).map { mix($0, $1, t) } }
      return row[0]
    }, radius: { t in radius * (1 - t) + tip * t })
    for end in [0, segments] {
      let center = UInt32(mesh.vertices.count)
      let p = end == 0 ? points[0] : points[points.count - 1]
      let normal = end == 0 ? normalize(points[0] - points[1])
        : normalize(points[points.count - 1] - points[points.count - 2])
      mesh.vertices.append(Vertex(p, normal, V3(repeating: 1)))
      for side in 0..<sides {
        let a = UInt32(end * (sides + 1) + side), b = a + 1
        mesh.indices += end == 0 ? [center, b, a] : [center, a, b]
      }
    }
    return mesh
  }
  private static func smooth(_ low: Float, _ high: Float, _ x: Float) -> Float {
    let t = min(1, max(0, (x - low) / (high - low))); return t * t * (3 - 2 * t)
  }
  private static func mix(_ a: V3, _ b: V3, _ t: Float) -> V3 { a * (1 - t) + b * t }
}
