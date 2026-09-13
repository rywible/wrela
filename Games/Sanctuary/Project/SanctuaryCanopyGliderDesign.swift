import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import simd

/// Small gliding mammal in metres. The patagium spans a curved leading support and
/// a concave trailing edge; it is not a flapping bird wing. Stable legacy part IDs
/// remain editable, and every attachment is expressed in the same bind frame.
enum SanctuaryCanopyGliderDesign {
  typealias Part = LivingWorldPresentation.PartDesign
  static let prefix = "Wildlife canopyGlider "
  static let headPivot = V3(0, 0.44, -0.28)
  static let mantlePivot = V3(0, 0.35, 0)
  // Source-space union of bind and the studio flight root (+.60 m, bob ±.09 m).
  // Qualified against registered root × articulated meshes, not camera padding.
  static let motionEnvelope = Bounds(V3(-0.77, 0.14, -0.72), V3(0.77, 1.42, 1.12))
  static let controls = [
    ScalarControl("muzzleWidth", "Soft muzzle width", 1, 0.9...1.1),
    ScalarControl("wingCamber", "Membrane camber", 1, 0.8...1.15),
    ScalarControl("eyeSize", "Eye aperture", 1, 0.9...1.1),
  ]
  private final class Cache: @unchecked Sendable {
    let lock = NSLock()
    var entries: [[UInt32]: [Part]] = [:]
    var order: [[UInt32]] = []
    func get(_ parameters: [String: Float]) -> [Part] {
      let values = SanctuaryCanopyGliderDesign.controls.map { c -> Float in
        let v = parameters[c.key] ?? c.initial
        return v.isFinite ? clamp(v, c.range.lowerBound, c.range.upperBound) : c.initial
      }
      let key = values.map(\.bitPattern)
      lock.lock(); defer { lock.unlock() }
      if let result = entries[key] { return result }
      let result = SanctuaryCanopyGliderDesign.make(muzzle: values[0], camber: values[1], eyeSize: values[2])
      if order.count == 4 { entries.removeValue(forKey: order.removeFirst()) }
      order.append(key); entries[key] = result
      return result
    }
  }
  private static let cache = Cache()
  static func parts(_ parameters: [String: Float] = [:]) -> [Part] { cache.get(parameters) }

  static func headEnvelope(muzzle: Float = 1) -> SectionLoft {
    SectionLoft([
      section("nose", -0.642, 0.466, 0.012, 0.012),
      section("muzzle", -0.601, 0.465, 0.075 * muzzle, 0.053),
      section("cheek", -0.512, 0.49, 0.166, 0.132),
      section("brow", -0.405, 0.503, 0.195, 0.164),
      section("poll", -0.29, 0.49, 0.155, 0.147),
      section("neck", -0.204, 0.443, 0.07, 0.08),
      section("neck-root", -0.18, 0.429, 0.015, 0.022),
    ])
  }
  private static func section(_ id: String, _ z: Float, _ y: Float,
    _ width: Float, _ height: Float) -> LoftSection {
    LoftSection(id: id, z: z, center: SIMD2(0, y), radius: SIMD2(width, height))
  }

  static func bodyEnvelope() -> SectionLoft {
    SectionLoft([
      section("throat", -0.42, 0.411, 0.045, 0.062),
      section("shoulder", -0.22, 0.367, 0.165, 0.172),
      section("back", 0.04, 0.338, 0.192, 0.17),
      section("haunch", 0.28, 0.307, 0.165, 0.142),
      section("rump", 0.43, 0.29, 0.075, 0.08),
      section("tail-seat", 0.49, 0.283, 0.012, 0.018),
    ])
  }

  private static func make(muzzle: Float, camber: Float, eyeSize: Float) -> [Part] {
    let coat = V3(0.44, 0.37, 0.28), cream = V3(0.69, 0.62, 0.47)
    let head = headEnvelope(muzzle: muzzle)
    let torso = bodyEnvelope()
    func part(_ id: String, _ mesh: Mesh, role: LivingWorldPresentation.CreatureRole = .body,
      pivot: V3 = .zero, parent: String? = nil, roughness: Float = 0.86) -> Part {
      let lo = mesh.vertices.map { xyz($0.position) }.reduce(V3(repeating: .infinity), simd_min)
      let hi = mesh.vertices.map { xyz($0.position) }.reduce(V3(repeating: -.infinity), simd_max)
      let bounds = Shape.box(simd_max((hi - lo) * 0.5, V3(repeating: 0.001))).moved((lo + hi) * 0.5)
      return Part(name: prefix + id, shape: bounds, color: V3(repeating: 1), material: 7,
        roughness: roughness, role: role, pivot: pivot, proceduralMesh: mesh, parent: parent)
    }
    var body = loft(torso, rings: 36, sides: 28) { p in
      let belly = smooth(0.30, 0.19, p.y)
      return mix(coat, cream, belly * 0.68)
    }
    let feet = [-1, 1].map { side in
      ParametricMesh.ellipsoid(V3(Float(side) * 0.135, 0.205, 0.355),
        V3(0.061, 0.048, 0.118), color: coat * 0.84, detail: 12)
    }
    body = ParametricMesh.joined([body] + feet)
    let skull = loft(head, rings: 40, sides: 32) { p in
      let cheek = smooth(0.535, 0.46, p.y)
      let crown = smooth(0.08, 0.018, abs(p.x)) * smooth(0.52, 0.58, p.y)
      return mix(mix(coat * 1.13, cream, cheek * 0.78), coat * 0.71, crown * 0.45)
    }
    var result = [part("body", body),
      part("head", skull, role: .head, pivot: headPivot, parent: prefix + "body")]
    var eyes: [Mesh] = []
    var creases: [Mesh] = []
    for side: Float in [-1, 1] {
      // Lens perimeter lies on the authored skull, rather than a detached globe.
      // The centre rises only 4 mm; the whole patch inherits the head's transform.
      let lens = ParametricMesh.surface(u: 20, v: 16, color: V3(0.022, 0.027, 0.024)) { u, v in
        let angle = u * 2 * Float.pi
        let radius = v
        let z = -0.502 + cos(angle) * radius * 0.038 * eyeSize
        let theta = side * (0.91 + sin(angle) * radius * 0.205 * eyeSize)
        let p = head.profile(at: z).value
        let surface = V3(sin(theta) * p.z, p.y + cos(theta) * p.w, z)
        let normal = normalize(head.sample(at: surface).gradient)
        return surface + normal * (0.0008 + 0.004 * (1 - radius * radius))
      }
      eyes.append(seatedNormals(lens, head: head))
      // The old Y-squash crosses the curved skull during closure. A tiny crease
      // stays on the same source surface; the open lens completely covers it.
      // It inherits the head, not the eye squash, and adds no open-eye rim.
      let crease = ParametricMesh.surface(u: 20, v: 4, color: V3(0.022, 0.027, 0.024)) { u, v in
        let z: Float = -0.502 + (2 * u - 1) * 0.030 * eyeSize
        let taper: Float = sin(u * .pi)
        let theta: Float = side * (0.91 + (2 * v - 1) * 0.014 * eyeSize * taper)
        let p = head.profile(at: z).value
        let surface = V3(sin(theta) * p.z, p.y + cos(theta) * p.w, z)
        return surface + normalize(head.sample(at: surface).gradient) * 0.0011
      }
      creases.append(seatedNormals(crease, head: head))
      let ear = ParametricMesh.ellipsoid(V3(side * 0.164, 0.589, -0.291),
        V3(0.068, 0.071, 0.035), color: coat * 0.95, detail: 16)
      result.append(part("ear \(side)", ear, role: .ear(side),
        pivot: V3(side * 0.145, 0.545, -0.29), parent: prefix + "head"))
    }
    result.append(part("eyes", ParametricMesh.joined(eyes), role: .eyes,
      parent: prefix + "head", roughness: 0.24))
    result.append(part("eyelid creases", ParametricMesh.joined(creases),
      parent: prefix + "head", roughness: 0.24))
    result.append(part("nose", ParametricMesh.ellipsoid(V3(0, 0.476, -0.641),
      V3(0.032, 0.019, 0.013), color: V3(0.085, 0.069, 0.059), detail: 12),
      parent: prefix + "head", roughness: 0.48))
    let wings = [-1, 1].map { wing(side: Float($0), camber: camber) }
    result.append(part("glider mantle", ParametricMesh.joined(wings), role: .expressive,
      pivot: mantlePivot, parent: prefix + "body", roughness: 0.91))
    let tail = ParametricMesh.tube(segments: 32, sides: 16, center: { t in
      V3(0.035 * sin(t * .pi), 0.287 - 0.025 * t + 0.055 * t * t, 0.36 + 0.70 * t)
    }, radius: { t in 0.059 * (1 - t * t) + 0.002 }, color: coat)
    result.append(part("ribbon tail", tail, role: .expressive,
      pivot: V3(0, 0.287, 0.36), parent: prefix + "body"))
    return result
  }

  /// Two closed cross-section membranes rooted 7 cm inside the shoulder. The
  /// thicker leading edge is the supporting forelimb; the trailing edge narrows
  /// smoothly toward a wrist, instead of forming the old oval saucer.
  private static func wing(side: Float, camber: Float) -> Mesh {
    var mesh = ParametricMesh.surface(u: 36, v: 28, position: { u, v in
      let angle = v * 2 * Float.pi
      let x: Float = 0.045 + 0.667 * u
      let chord: Float = 0.335 * pow(1 - u, 0.65) + 0.002
      let centerZ: Float = 0.038 - 0.13 * u
      let arch: Float = 0.065 * sin(u * .pi) * camber
      let centerY: Float = 0.347 + arch + 0.018 * u
      let leadingSupport: Float = 0.020 * pow(max(0, -cos(angle)), 8) * (1 - u)
      let depth: Float = 0.010 + leadingSupport
      return V3(side * x, centerY + sin(angle) * depth, centerZ + cos(angle) * chord)
    }, tint: { u, v in
      let lead = pow(max(0, -cos(v * 2 * .pi)), 8)
      let upper = max(0, sin(v * 2 * .pi))
      return mix(V3(0.57, 0.43, 0.29), V3(0.38, 0.32, 0.25), lead * 0.66)
        + V3(repeating: upper * 0.025 - u * 0.025)
    })
    if side < 0 {
      for i in mesh.vertices.indices { mesh.vertices[i].normal = -mesh.vertices[i].normal }
      for i in stride(from: 0, to: mesh.indices.count, by: 3) { mesh.indices.swapAt(i + 1, i + 2) }
    }
    return mesh
  }

  private static func loft(_ source: SectionLoft, rings: Int, sides: Int,
    color: @escaping (V3) -> V3) -> Mesh {
    let lo = source.sections.first!.z, hi = source.sections.last!.z
    func point(_ u: Float, _ v: Float) -> V3 {
      let z = lo + (hi - lo) * v, p = source.profile(at: z).value
      let a = u * 2 * Float.pi
      return V3(cos(a) * p.z, p.y + sin(a) * p.w, z)
    }
    var mesh = ParametricMesh.surface(u: sides, v: rings, position: point,
      tint: { u, v in color(point(u, v)) })
    for end in [0, rings] {
      let z = end == 0 ? lo : hi, p = source.profile(at: z).value
      let index = UInt32(mesh.vertices.count), center = V3(0, p.y, z)
      mesh.vertices.append(Vertex(center, V3(0, 0, end == 0 ? -1 : 1), color(center)))
      for i in 0..<sides {
        let a = UInt32(end * (sides + 1) + i), b = a + 1
        mesh.indices += end == 0 ? [index, b, a] : [index, a, b]
      }
    }
    return mesh
  }

  private static func seatedNormals(_ source: Mesh, head: SectionLoft) -> Mesh {
    var result = source
    for i in result.vertices.indices {
      let p = xyz(result.vertices[i].position)
      result.vertices[i].normal = SIMD4(normalize(head.sample(at: p).gradient), 0)
    }
    for i in stride(from: 0, to: result.indices.count, by: 3) {
      let a = result.vertices[Int(result.indices[i])]
      let b = result.vertices[Int(result.indices[i + 1])]
      let c = result.vertices[Int(result.indices[i + 2])]
      if dot(cross(xyz(b.position - a.position), xyz(c.position - a.position)), xyz(a.normal)) < 0 {
        result.indices.swapAt(i + 1, i + 2)
      }
    }
    return result
  }

  private static func xyz(_ value: SIMD4<Float>) -> V3 { V3(value.x, value.y, value.z) }

  private static func smooth(_ low: Float, _ high: Float, _ value: Float) -> Float {
    let t = clamp((value - low) / (high - low), 0, 1)
    return t * t * (3 - 2 * t)
  }
  private static func mix(_ a: V3, _ b: V3, _ amount: Float) -> V3 {
    a * (1 - amount) + b * amount
  }

  static func poses(time: Float, gaze: Float, mood: String) -> [String: JointPose] {
    let blink = CreatureMotion().blink(at: time)
    let attention = clamp(gaze, -16, 16)
    // Small pitch/bank tension in a gliding membrane, not an in-place powered gait.
    return [
      prefix + "head": JointPose(rotation: V3(sin(time * 0.7) * 1.5, attention, 0)),
      prefix + "eyes": JointPose(scale: V3(1, max(0.06, 1 - blink * 0.94), 1)),
      prefix + "glider mantle": JointPose(rotation: V3(sin(time * 0.62) * 1.2, 0,
        sin(time * 0.41) * 1.4)),
      prefix + "ribbon tail": JointPose(rotation: V3(sin(time * 0.62 - 0.7) * 3,
        sin(time * 0.41 - 0.4) * 3, 0)),
    ]
  }
}
