import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import simd

/// One small, seated woodland hare: a pear-shaped torso, tucked haunches,
/// supporting forepaws and a cheek-led face. Metres, Y up, forward -Z.
/// These fields and semantic attachments are shared by the game and Soundstage.
enum SanctuarySunhareDesign {
  typealias Part = LivingWorldPresentation.PartDesign
  typealias Role = LivingWorldPresentation.CreatureRole
  static let controls: [ScalarControl] = [
    .init("headWidth", "Face breadth", 1, 0.88...1.15),
    .init("cheeks", "Cheek softness", 1, 0.85...1.15),
    .init("earLength", "Ear length · m", 0.39, 0.28...0.50),
    .init("earSpread", "Ear spread · °", 13, 8...25),
    .init("bodyRoundness", "Haunch roundness", 1, 0.85...1.15),
    .init("pawSize", "Paw breadth", 1, 0.85...1.15),
    .init("coatStudy", "Coat study · 0 reference / 1 candidate", 0, 0...1),
    .init("furStudy", "Silhouette fur study · 0 off / 1 on", 0, 0...1),
  ]
  private struct Parameters: Hashable {
    var headWidth, cheeks, earLength, earSpread, bodyRoundness, pawSize: Float
    var coatStudy, furStudy: Bool
    init(_ p: [String: Float]) {
      headWidth = p["headWidth"] ?? 1; cheeks = p["cheeks"] ?? 1
      earLength = p["earLength"] ?? 0.39; earSpread = p["earSpread"] ?? 13
      bodyRoundness = p["bodyRoundness"] ?? 1; pawSize = p["pawSize"] ?? 1
      // The candidate is explicit; omitted values and intermediate slider
      // positions retain the reference rather than blending material families.
      coatStudy = p["coatStudy"] == 1
      furStudy = p["furStudy"] == 1
    }
  }
  private final class Recipes: @unchecked Sendable {
    struct Entry {
      let base: [Part]
      let metadata: [Part]
      var compiled: Result<[Part], Error>?
      var coatBytes = 0
    }
    let lock = NSLock()
    var values: [Parameters: Entry] = [:]
    private func entry(_ parameters: Parameters) -> Entry {
      if let existing = values[parameters] { return existing }
      let base = build(parameters)
      let result = Entry(base: base, metadata: parameters.furStudy
        ? base + SanctuarySunhareCoatDesign.metadata(base: base) : base)
      if values.count >= SanctuarySunhareCoatDesign.maximumRecipes { values.removeAll(keepingCapacity: true) }
      values[parameters] = result
      return result
    }
    func get(_ parameters: Parameters) -> [Part] {
      lock.lock(); defer { lock.unlock() }
      return entry(parameters).metadata
    }
    func compile(_ parameters: Parameters) throws -> [Part] {
      lock.lock(); defer { lock.unlock() }
      var value = entry(parameters)
      guard parameters.furStudy else { return value.base }
      if let result = value.compiled { return try result.get() }
      let result: Result<[Part], Error> = Result {
        let coat = try SanctuarySunhareCoatDesign.compile(base: value.base)
        let retained = values.values.reduce(0) { $0 + $1.coatBytes }
        guard retained + coat.meshBytes <= SanctuarySunhareCoatDesign.maximumCacheBytes else {
          throw RuntimeError.message("Sunhare coat: recipe cache exceeds its storage budget")
        }
        value.coatBytes = coat.meshBytes
        return value.base + coat.parts
      }
      value.compiled = result
      values[parameters] = value
      return try result.get()
    }
    func statistics() -> (recipes: Int, coatBytes: Int) {
      lock.lock(); defer { lock.unlock() }
      return (values.count, values.values.reduce(0) { $0 + $1.coatBytes })
    }
  }
  private static let recipes = Recipes()
  static func parts(_ parameters: [String: Float] = [:]) -> [Part] {
    recipes.get(Parameters(parameters))
  }
  /// Called by the generator's throwing compile path. Catalog metadata remains
  /// available without compiling; an invalid groom is never silently omitted.
  static func validatedParts(_ parameters: [String: Float] = [:]) throws -> [Part] {
    for (key, value) in parameters {
      guard let control = controls.first(where: { $0.key == key }), value.isFinite,
        control.range.contains(value), key != "furStudy" || value == 0 || value == 1 else {
        throw RuntimeError.message("Invalid Sunhare source parameter: " + key)
      }
    }
    return try recipes.compile(Parameters(parameters))
  }
  static var cacheStatistics: (recipes: Int, coatBytes: Int) { recipes.statistics() }
  /// The UV finite-difference helper's absolute tangent cutoff is too large
  /// for millimetre eye caps and near ellipsoid poles. Analytic gradients are
  /// scale-independent and agree on both copies of the longitude seam.
  private static func ellipsoidMesh(_ center: V3, _ radii: V3, detail: Int) -> Mesh {
    var mesh = ParametricMesh.ellipsoid(center, radii, detail: detail)
    for i in mesh.vertices.indices {
      let vertex = mesh.vertices[i].position
      let local = V3(vertex.x, vertex.y, vertex.z) - center
      mesh.vertices[i].normal = SIMD4(normalize(local / (radii * radii)), 0)
    }
    return mesh
  }

  /// Material regions share a boundary but never a triangle. Compaction also
  /// keeps each semantic part's bounds tied to its own rendered surface.
  private static func partition(_ mesh: Mesh, selected: [Bool]) -> (outer: Mesh, inner: Mesh) {
    var outer = Mesh(), inner = Mesh()
    var outerMap: [UInt32: UInt32] = [:], innerMap: [UInt32: UInt32] = [:]
    func append(_ ids: ArraySlice<UInt32>, to target: inout Mesh, map: inout [UInt32: UInt32]) {
      for id in ids {
        if let existing = map[id] { target.indices.append(existing) }
        else {
          let next = UInt32(target.vertices.count)
          target.vertices.append(mesh.vertices[Int(id)]); map[id] = next; target.indices.append(next)
        }
      }
    }
    for i in stride(from: 0, to: mesh.indices.count, by: 3) {
      let ids = mesh.indices[i..<(i + 3)]
      if ids.contains(where: { selected[Int($0)] }) { append(ids, to: &inner, map: &innerMap) }
      else { append(ids, to: &outer, map: &outerMap) }
    }
    return (outer, inner)
  }

  private static func build(_ p: Parameters) -> [Part] {
    let prefix = "Wildlife sunhare "
    let amber = V3(0.57, 0.34, 0.15)
    let faceColor = V3(0.70, 0.46, 0.23)
    let cream = V3(0.83, 0.69, 0.46)
    let earColor = V3(0.56, 0.31, 0.27)
    let coatParts: Set<String> = ["body", "head", "ear-left", "ear-right",
      "front-left", "front-right", "hind-left", "hind-right"]
    func ellipsoid(_ center: V3, _ radii: V3) -> Shape {
      Shape.sphere(1).stretched(radii).moved(center)
    }
    func part(_ suffix: String, _ shape: Shape, _ color: V3,
      role: Role = .body, pivot: V3 = .zero, parent: String? = nil,
      material: Int = 9, roughness: Float = 0.86, resolution: Int = 32, mesh: Mesh? = nil) -> Part
    {
      let selectedMaterial = p.coatStudy && material == 9 && coatParts.contains(suffix) ? 15 : material
      var value = Part(name: prefix + suffix, shape: shape, color: color,
        material: selectedMaterial, roughness: roughness, role: role, pivot: pivot)
      value.parent = parent.map { prefix + $0 }; value.resolution = resolution
      value.proceduralMesh = mesh
      return value
    }

    // The neck overlaps the lower skull throughout the bounded social turn.
    // Haunches are part of the torso; the sole-bearing paws remain root children.
    let body = ellipsoid(V3(0, 0.355, 0.21), V3(0.255 * p.bodyRoundness, 0.29, 0.325))
      .blended(ellipsoid(V3(0, 0.405, -0.12), V3(0.205, 0.255, 0.255)), radius: 0.07)
      .blended(ellipsoid(V3(0, 0.575, -0.235), V3(0.158, 0.18, 0.15)), radius: 0.055)
    let headCenter = V3(0, 0.755, -0.315)
    let headRadii = V3(0.252 * p.headWidth, 0.224, 0.227)
    let headPivot = V3(0, 0.63, -0.25)
    var head = ellipsoid(headCenter, headRadii)
    for side: Float in [-1, 1] {
      head = head.blended(ellipsoid(V3(side * 0.116 * p.headWidth, 0.655, -0.402),
        V3(0.131 * p.cheeks, 0.103, 0.122)), radius: 0.035)
    }
    let muzzle = ellipsoid(V3(-0.041, 0.644, -0.506), V3(0.059, 0.040, 0.044))
      .blended(ellipsoid(V3(0.041, 0.644, -0.506), V3(0.059, 0.040, 0.044)), radius: 0.016)
    let nose = ellipsoid(V3(0, 0.670, -0.540), V3(0.024, 0.016, 0.012))
      .blended(ellipsoid(V3(0, 0.660, -0.542), V3(0.013, 0.013, 0.010)), radius: 0.004)

    func eye(_ side: Float) -> (shape: Shape, mesh: Mesh) {
      let x: Float = side * 0.139 * p.headWidth
      let y: Float = 0.790
      let nx = x / headRadii.x, ny = (y - headCenter.y) / headRadii.y
      let z = headCenter.z - headRadii.z * sqrt(max(0, 1 - nx * nx - ny * ny))
      let surface = V3(x, y, z)
      let normal = normalize((surface - headCenter) / (headRadii * headRadii))
      let orientation = simd_quatf(from: V3(0, 0, 1), to: normal)
      let center = surface - normal * 0.0045
      let radii = V3(0.033, 0.038, 0.010)
      let shape = ellipsoid(.zero, radii).rotated(orientation).moved(center)
      // A warm iris surrounds a dark pupil on the shallow corneal cap. These
      // are material colors, not painted highlights; specular response remains
      // driven by the shared scene lights and this surface's real normals.
      var mesh = ellipsoidMesh(.zero, radii, detail: 24)
      for i in mesh.vertices.indices {
        let local = V3(mesh.vertices[i].position.x, mesh.vertices[i].position.y,
          mesh.vertices[i].position.z)
        let aperture = length(SIMD2(local.x / radii.x, local.y / radii.y))
        let pupil = length(SIMD2(local.x / 0.019, (local.y - 0.002) / 0.026))
        let iris = CreatureMotion.smooth((pupil - 0.88) / 0.25)
          * (1 - CreatureMotion.smooth((aperture - 0.86) / 0.13))
        let dark = V3(0.023, 0.020, 0.016)
        let amberIris = V3(0.27, 0.15, 0.067)
        mesh.vertices[i].color = SIMD4(dark + (amberIris - dark) * iris, 1)
        let n = V3(mesh.vertices[i].normal.x, mesh.vertices[i].normal.y, mesh.vertices[i].normal.z)
        mesh.vertices[i].position = SIMD4(center + orientation.act(local), 1)
        mesh.vertices[i].normal = SIMD4(orientation.act(n), 0)
      }
      return (shape, mesh)
    }
    let leftEye = eye(-1), rightEye = eye(1)
    var result: [Part] = [
      part("body", body, amber, pivot: V3(0, 0.28, 0.02)),
      part("head", head, faceColor, role: .head, pivot: headPivot, parent: "body", resolution: 32),
      part("muzzle", muzzle, cream, parent: "head", resolution: 24),
      part("nose", nose, V3(0.21, 0.115, 0.085), parent: "head", material: 7, roughness: 0.38, resolution: 16),
      part("eyes", leftEye.shape.joined(rightEye.shape), V3(repeating: 1),
        role: .eyes, parent: "head", material: 7, roughness: 0.18,
        mesh: ParametricMesh.joined([leftEye.mesh, rightEye.mesh])),
      part("tail", ellipsoid(V3(0, 0.34, 0.505), V3(0.115, 0.113, 0.115)), cream,
        role: .expressive, pivot: V3(0, 0.32, 0.44), parent: "body",
        mesh: ellipsoidMesh(V3(0, 0.34, 0.505), V3(0.115, 0.113, 0.115), detail: 20)),
    ]
    for (index, side) in [Float(-1), Float(1)].enumerated() {
      let suffix = side < 0 ? "left" : "right"
      let foreX = side * 0.143
      // Bury the shoulder well inside the chest and taper toward the
      // planted wrist. The former exposed upper ellipsoid read as a separate egg.
      let fore = Shape.capsule(V3(side * 0.110, 0.43, -0.19),
        V3(foreX, 0.095, -0.25), 0.055)
        .blended(ellipsoid(V3(foreX, 0.056, -0.266),
          V3(0.083 * p.pawSize, 0.056, 0.128 * p.pawSize)), radius: 0.024)
      // Both soles are authored at exactly Y=0 and kept outside body crouch.
      let hind = ellipsoid(V3(side * 0.204, 0.080, 0.187),
        V3(0.117 * p.pawSize, 0.080, 0.190 * p.pawSize))
        .blended(ellipsoid(V3(side * 0.204, 0.20, 0.268),
          V3(0.126, 0.17, 0.166)), radius: 0.035)
      result.append(part("front-\(suffix)", fore, V3(0.61, 0.38, 0.18),
        role: .leg(index), pivot: V3(foreX, 0.056, -0.25), resolution: 32))
      result.append(part("hind-\(suffix)", hind, amber,
        role: .leg(index + 2), pivot: V3(side * 0.204, 0.08, 0.19), resolution: 32))
      let earLength = p.earLength * (side < 0 ? 0.96 : 1.04)
      let angle = -side * p.earSpread * .pi / 180
      let lean = simd_quatf(angle: angle, axis: V3(0, 0, 1))
      let earRoot = V3(side * 0.115 * p.headWidth, 0.915, -0.282)
      let ear = ellipsoid(V3(0, earLength * 0.48, 0), V3(0.073, earLength * 0.56, 0.043))
        .rotated(lean).moved(earRoot)
      // Outer fur and lining partition the SAME tessellated lamina. Every
      // triangle belongs to exactly one part; shared seam positions/normals
      // eliminate coplanar overlap without pushing a lining in front of the ear.
      var lamina = ellipsoidMesh(V3(0, earLength * 0.48, 0),
        V3(0.073, earLength * 0.56, 0.043), detail: 28)
      var lining = [Bool](repeating: false, count: lamina.vertices.count)
      for i in lamina.vertices.indices {
        let local = V3(lamina.vertices[i].position.x, lamina.vertices[i].position.y,
          lamina.vertices[i].position.z)
        let distance = length(SIMD2(local.x / 0.049,
          (local.y - earLength * 0.51) / (earLength * 0.435)))
        let pigment: Float = local.z < 0 ? 1 - CreatureMotion.smooth((distance - 0.88) / 0.15) : 0
        lining[i] = pigment > 0
        lamina.vertices[i].color = SIMD4(faceColor + (earColor - faceColor) * pigment, 1)
        let n = V3(lamina.vertices[i].normal.x, lamina.vertices[i].normal.y,
          lamina.vertices[i].normal.z)
        lamina.vertices[i].position = SIMD4(lean.act(local) + earRoot, 1)
        lamina.vertices[i].normal = SIMD4(lean.act(n), 0)
      }
      let meshes = partition(lamina, selected: lining)
      result.append(part("ear-\(suffix)", ear, V3(repeating: 1), role: .ear(side),
        pivot: earRoot, parent: "head", mesh: meshes.outer))
      result.append(part("inner-ear-\(suffix)", ear, V3(repeating: 1),
        parent: "ear-\(suffix)", mesh: meshes.inner))
    }
    return result
  }
}
