import FieldCompiler
import FieldCore
import Foundation
import simd

/// Editable field source, not baked geometry. Shared by games and the workshop.
package struct FieldExpression: Codable {
  package var op: String
  package var values: [Float] = []
  package var children: [FieldExpression] = []
  package init(op: String, values: [Float] = [], children: [FieldExpression] = []) {
    self.op = op
    self.values = values
    self.children = children
  }
  package var nodeCount: Int { 1 + children.reduce(0) { $0 + $1.nodeCount } }
  package func shape(depth: Int = 0) throws -> Shape {
    guard depth < 24, values.allSatisfy(\.isFinite), values.allSatisfy({ abs($0) <= 100 }),
      children.count <= 2
    else { throw RuntimeError.message("Field exceeds depth, child or coordinate limits") }
    func require(_ n: Int, _ c: Int) throws {
      guard values.count == n, children.count == c else {
        throw RuntimeError.message("Invalid arguments for field operation \(op)")
      }
    }
    func vector(_ offset: Int = 0) -> V3 {
      V3(values[offset], values[offset + 1], values[offset + 2])
    }
    let result: Shape
    switch op {
    case "sphere":
      try require(1, 0)
      result = .sphere(values[0])
    case "box":
      try require(3, 0)
      result = .box(vector())
    case "capsule":
      try require(7, 0)
      result = .capsule(vector(), vector(3), values[6])
    case "torus":
      try require(2, 0)
      result = .torus(values[0], values[1])
    case "move":
      try require(3, 1)
      result = try children[0].shape(depth: depth + 1).moved(vector())
    case "stretch":
      try require(3, 1)
      result = try children[0].shape(depth: depth + 1).stretched(vector())
    case "scale":
      try require(1, 1)
      result = try children[0].shape(depth: depth + 1).sized(values[0])
    case "union", "subtract", "blend":
      try require(op == "blend" ? 1 : 0, 2)
      let a = try children[0].shape(depth: depth + 1)
      let b = try children[1].shape(depth: depth + 1)
      result =
        op == "union"
        ? a.joined(b) : (op == "subtract" ? a.cut(b) : a.blended(b, radius: values[0]))
    default: throw RuntimeError.message("Unknown field operation \(op)")
    }
    try result.validate()
    return result
  }
}
package struct AssetPart: Codable {
  package var name: String
  package var joint: PartJoint?
  package init(
    name: String, joint: PartJoint? = nil, field: FieldExpression, color: [Float],
    material: Int = 7, roughness: Float = 0.7, metallic: Float = 0
  ) {
    self.name = name
    self.joint = joint
    self.field = field
    self.color = color
    self.material = material
    self.roughness = roughness
    self.metallic = metallic
  }
  package var key: String { joint?.id ?? name }
  package var field: FieldExpression
  package var color: [Float]
  package var material: Int = 7
  package var roughness: Float = 0.7
  package var metallic: Float = 0
}
package struct PartOverride: Codable {
  package init(joint: PartJoint? = nil, roughness: Float? = nil, metallic: Float? = nil) {
    self.joint = joint
    self.roughness = roughness
    self.metallic = metallic
  }
  package var joint: PartJoint?
  package var roughness: Float?
  package var metallic: Float?
}
package struct AssetSource: Codable {
  package init(
    id: String, name: String, generator: String, parameters: [String: Float] = [:],
    parts: [AssetPart] = [], appearance: [String: Float] = [:], motion: MotionParameters? = nil,
    partOverrides: [String: PartOverride] = [:]
  ) {
    self.id = id
    self.name = name
    self.generator = generator
    self.parameters = parameters
    self.parts = parts
    self.appearance = appearance
    self.motion = motion
    self.partOverrides = partOverrides
  }
  package var version: Int = 1
  package var id: String
  package var name: String
  package var generator: String
  package var parameters: [String: Float] = [:]
  package var parts: [AssetPart] = []
  package var appearance: [String: Float] = [:]
  package var motion: MotionParameters?
  package var partOverrides: [String: PartOverride] = [:]
  package var definition: AssetGenerator? { ProjectContext.generator(generator) }
  package var controls: [ScalarControl] { definition?.controls ?? [] }
  package var animation: AnimationDefinition? { definition?.animation }
  package var shapeRanges: [String: ClosedRange<Float>] {
    Dictionary(uniqueKeysWithValues: controls.map { ($0.key, $0.range) })
  }
  package var resolvedParts: [AssetPart] {
    var result = definition?.parts(parameters).nilIfEmpty ?? parts
    for i in result.indices {
      if let override = partOverrides[result[i].key] {
        if let joint = override.joint { result[i].joint = joint }
        if let value = override.roughness { result[i].roughness = value }
        if let value = override.metallic { result[i].metallic = value }
      }
    }
    return result
  }
  package var joints: [PartJoint] { resolvedParts.map { $0.joint ?? PartJoint(id: $0.name) } }
  package static var availableIDs: [String] {
    ids
      + (((try? FileManager.default.contentsOfDirectory(
        at: url(ProjectContext.current.defaultSubject).deletingLastPathComponent(),
        includingPropertiesForKeys: nil)) ?? []).filter { $0.pathExtension == "json" }.map {
        $0.deletingPathExtension().lastPathComponent
      }.filter {
        !ids.contains($0)
      }).sorted()
  }
  package static var ids: [String] { ProjectContext.current.generators.map(\.id) }
  package static var workspace: URL { ProjectContext.workspace }
  package static func url(_ id: String) -> URL {
    ProjectContext.current.authoring.appendingPathComponent("Assets/\(id).json")
  }
  package static func defaults(_ id: String) -> AssetSource {
    let d = ProjectContext.generator(id)
    return AssetSource(
      id: id, name: d?.name ?? id, generator: id,
      parameters: Dictionary(
        uniqueKeysWithValues: (d?.controls ?? []).map { ($0.key, $0.initial) }),
      motion: d?.animation.map {
        MotionParameters(Dictionary(uniqueKeysWithValues: $0.controls.map { ($0.key, $0.initial) }))
      })
  }
  package static func load(_ id: String) throws -> AssetSource {
    guard availableIDs.contains(id) else { throw RuntimeError.message("Unknown subject \(id)") }
    if FileManager.default.fileExists(atPath: url(id).path) { return try read(url(id)) }
    return defaults(id)
  }
  package static func read(_ url: URL) throws -> AssetSource {
    let data = try Data(contentsOf: url)
    guard data.count < 262144 else {
      throw RuntimeError.message("Asset source must be smaller than 256 KiB")
    }
    let source = try JSONDecoder().decode(AssetSource.self, from: data)
    try source.validate()
    return source
  }
  package func validate() throws {
    guard version == 1, !id.isEmpty, id.count < 64,
      id.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") }),
      !name.isEmpty, name.count < 100
    else { throw RuntimeError.message("Invalid asset identity or version") }
    guard Self.ids.contains(generator) || generator == "fields" else {
      throw RuntimeError.message("Unknown asset generator")
    }
    for (key, value) in parameters {
      guard let range = shapeRanges[key], value.isFinite, range.contains(value) else {
        throw RuntimeError.message("Invalid shape parameter \(key)")
      }
    }
    for (key, value) in appearance {
      guard ["roughness", "metallic", "tint"].contains(key), value.isFinite,
        key == "tint" ? (0.3...1.5).contains(value) : (0...1).contains(value)
      else { throw RuntimeError.message("Invalid asset appearance") }
    }
    let generated = definition?.parts(parameters).nilIfEmpty ?? parts
    let ids = Set(generated.map(\.key))
    guard partOverrides.keys.allSatisfy(ids.contains),
      partOverrides.allSatisfy({ id, value in value.joint == nil || value.joint?.id == id })
    else {
      throw RuntimeError.message("Part overrides must name existing, stable anatomy IDs")
    }
    if let motion {
      guard let animation else {
        throw RuntimeError.message("This generator does not define animation")
      }
      try animation.validate(motion)
    }
    try PartRig.validate(joints)
    if definition?.compile == nil {
      let parts = resolvedParts
      guard (1...32).contains(parts.count), parts.reduce(0, { $0 + $1.field.nodeCount }) <= 256
      else {
        throw RuntimeError.message("A field asset needs 1...32 parts and at most 256 field nodes")
      }
      for part in parts {
        guard part.color.count == 3, part.color.allSatisfy({ $0.isFinite && (0...1).contains($0) }),
          (0...9).contains(part.material), (0.05...1).contains(part.roughness),
          (0...1).contains(part.metallic)
        else { throw RuntimeError.message("Invalid part material") }
        let shape = try part.field.shape()
        let b = shape.bounds
        guard length(b.max - b.min) <= 100, length(b.max - b.min) >= 0.001 else {
          throw RuntimeError.message("Field bounds must span 1 mm...100 m")
        }
      }
    }
  }
  package func compile() throws -> [SceneBatch] {
    try compileParts().map { part in
      var b = part
      b.doubleSided = b.instances.first?.tint.w == 8 || b.instances.first?.tint.w == 3
      if let rough = appearance["roughness"] { b.roughness = max(0.05, rough) }
      if let metal = appearance["metallic"] { b.metallic = metal }
      for i in b.instances.indices {
        let kind = b.instances[i].tint.w
        b.instances[i].tint *= appearance["tint"] ?? 1
        b.instances[i].tint.w = kind
      }
      return b
    }
  }
  private func compileParts() throws -> [SceneBatch] {
    try validate()
    func batch(
      _ name: String, _ shape: Shape, _ tint: V3, _ kind: Float, _ roughness: Float = -1,
      _ metallic: Float = 0
    ) throws -> SceneBatch {
      var b = SceneBatch(
        name: name, mesh: try Mesher.compile(shape, resolution: 48),
        instances: [Instance(tint: tint, kind: kind)])
      b.roughness = roughness
      b.metallic = metallic
      return b
    }
    if let compile = definition?.compile { return try compile(self) }
    return try resolvedParts.map { p in
      try batch(
        p.key, p.field.shape(), V3(p.color[0], p.color[1], p.color[2]), Float(p.material),
        p.roughness, p.metallic)
    }
  }

}

// Omitted optional JSON fields take the same defaults as Swift-authored fields.
extension FieldExpression {
  enum CodingKeys: String, CodingKey { case op, values, children }
  package init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    op = try c.decode(String.self, forKey: .op)
    values = try c.decodeIfPresent([Float].self, forKey: .values) ?? []
    children = try c.decodeIfPresent([Self].self, forKey: .children) ?? []
  }
}
extension AssetPart {
  enum CodingKeys: String, CodingKey {
    case name, joint, field, color, material, roughness, metallic
  }
  package init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    name = try c.decode(String.self, forKey: .name)
    joint = try c.decodeIfPresent(PartJoint.self, forKey: .joint)
    field = try c.decode(FieldExpression.self, forKey: .field)
    color = try c.decodeIfPresent([Float].self, forKey: .color) ?? [0.55, 0.55, 0.55]
    material = try c.decodeIfPresent(Int.self, forKey: .material) ?? 7
    roughness = try c.decodeIfPresent(Float.self, forKey: .roughness) ?? 0.7
    metallic = try c.decodeIfPresent(Float.self, forKey: .metallic) ?? 0
  }
}
extension AssetSource {
  enum CodingKeys: String, CodingKey {
    case version, id, name, generator, parameters, parts, appearance, motion, partOverrides
  }
  package init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    version = try c.decodeIfPresent(Int.self, forKey: .version) ?? 1
    id = try c.decode(String.self, forKey: .id)
    name = try c.decode(String.self, forKey: .name)
    generator = try c.decodeIfPresent(String.self, forKey: .generator) ?? "fields"
    parameters = try c.decodeIfPresent([String: Float].self, forKey: .parameters) ?? [:]
    parts = try c.decodeIfPresent([AssetPart].self, forKey: .parts) ?? []
    partOverrides =
      try c.decodeIfPresent([String: PartOverride].self, forKey: .partOverrides) ?? [:]
    motion = try c.decodeIfPresent(MotionParameters.self, forKey: .motion)
    appearance = try c.decodeIfPresent([String: Float].self, forKey: .appearance) ?? [:]
  }
}

extension Array { fileprivate var nilIfEmpty: Self? { isEmpty ? nil : self } }
