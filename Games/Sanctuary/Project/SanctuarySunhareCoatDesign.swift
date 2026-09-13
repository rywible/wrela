import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import simd

/// Opt-in short undercoat: overlapping conformal patches carry the established
/// filtered GroomCoverage representation. No shell stack, runtime simulation,
/// camera-facing geometry or new shader path. Bind-space recipes are source.
enum SanctuarySunhareCoatDesign {
  typealias Part = LivingWorldPresentation.PartDesign
  static let parents = ["body", "head", "ear-left", "ear-right"]
  static let prefix = "Wildlife sunhare "
  static let maximumTriangles = 16_896
  static let maximumMeshBytes = 1_048_576
  static let maximumCacheBytes = 4 * maximumMeshBytes
  static let maximumRecipes = 4
  static let fibresPerGuide = 1
  static let regionCounts = [220, 112, 10, 10]

  struct Guide {
    let parent: String
    let surface: V3
    let normal: V3
    let points: [V3]
    let outerTriangle: Int?
    var vertexStart: Int
    let vertexCount: Int
    let rootWidth: Int
    let interiorVertex: Int
    let tipVertex: Int
  }
  struct Recipe {
    let parts: [Part]
    let guides: [Guide]
    let meshBytes: Int
    let maximumReductionError: Float
    let exposedVertexFraction: Float
  }
  private struct Seat {
    let point: V3
    let normal: V3
    let triangle: Int?
  }
  private struct Patch {
    let mesh: Mesh
    let guide: Guide
    let error: Float
  }

  /// Metadata and pose queries never author meshes or suppress compile errors.
  static func metadata(base: [Part]) -> [Part] {
    parents.compactMap { suffix in
      guard let parent = base.first(where: { $0.name == prefix + suffix }) else { return nil }
      let bounds = parent.shape.bounds
      return Part(name: prefix + "coat " + suffix,
        shape: .box((bounds.max - bounds.min) * 0.5 + V3(repeating: 0.05))
          .moved((bounds.min + bounds.max) * 0.5),
        color: V3(repeating: 1), material: 13, roughness: 0.86,
        castsShadow: false, parent: parent.name)
    }
  }

  private static func require(_ condition: Bool, _ message: String) throws {
    guard condition else { throw RuntimeError.message("Sunhare coat: " + message) }
  }
  private static func v3(_ p: SIMD4<Float>) -> V3 { V3(p.x, p.y, p.z) }
  private static func finite(_ p: V3) -> Bool { (0..<3).allSatisfy { p[$0].isFinite } }

  /// The supported parent fields are star-shaped about these bounds centers.
  /// Every projection brackets the actual field; invalid source is an error.
  private static func project(_ part: Part, toward target: V3) throws -> Seat {
    let origin = (part.shape.bounds.min + part.shape.bounds.max) * 0.5
    let offset = target - origin
    try require(finite(offset) && length_squared(offset) > 1e-10, "invalid surface projection direction")
    let direction = normalize(offset)
    var near: Float = 0, far: Float = 2
    try require(part.shape.value(at: origin) < 0 && part.shape.value(at: origin + direction * far) > 0,
      "projection must bracket " + part.name)
    for _ in 0..<32 {
      let middle = (near + far) * 0.5
      if part.shape.value(at: origin + direction * middle) < 0 { near = middle }
      else { far = middle }
    }
    let surface = origin + direction * ((near + far) * 0.5)
    let normal = part.shape.normal(at: surface)
    try require(finite(surface) && finite(normal) && length_squared(normal) > 0.9,
      "nonfinite projected surface or normal")
    return Seat(point: surface, normal: normalize(normal), triangle: nil)
  }

  private static func seats(_ part: Part, ear: Bool, head: Bool) throws -> [Seat] {
    if ear {
      guard let mesh = part.proceduralMesh else { throw RuntimeError.message("Sunhare coat: missing outer ear mesh") }
      var result: [Seat] = []
      for triangle in stride(from: 0, to: mesh.indices.count, by: 3) {
        let vertices = (0..<3).map { mesh.vertices[Int(mesh.indices[triangle + $0])] }
        let point = vertices.map { v3($0.position) }.reduce(V3.zero, +) / 3
        let normal = normalize(vertices.map { v3($0.normal) }.reduce(V3.zero, +))
        if point.z > part.pivot.z + 0.026 && normal.z > 0.8 && point.y > part.pivot.y + 0.055 {
          result.append(Seat(point: point, normal: normal, triangle: triangle / 3))
        }
      }
      return result
    }
    let origin = (part.shape.bounds.min + part.shape.bounds.max) * 0.5
    var result: [Seat] = []
    for i in 0..<1536 {
      let y = 1 - 2 * (Float(i) + 0.5) / 1536
      let radius = sqrt(max(0, 1 - y * y))
      let angle = Float(i) * 2.3999632 + (head ? 0.371 : 0.113)
      let direction = V3(radius * cos(angle), y, radius * sin(angle))
      let seat = try project(part, toward: origin + direction)
      if seat.point.y > (head ? 0.64 : 0.35) { result.append(seat) }
    }
    return result
  }

  /// Each patch overlaps its neighbours and resolves many subpixel fibres via
  /// GroomCoverage. Lift stays millimetric; the old macroscopic curled bristles
  /// are not retained. Source-field projections conform the entire patch width,
  /// including its buried root edge, to the actual parent anatomy.
  private static func patch(_ parent: Part, seat: Seat, ear: Bool, head: Bool,
    number: Int, exclusions: [Part]) throws -> Patch? {
    let side: Float = parent.name.hasSuffix("left") ? -1 : 1
    let comb = ear ? V3(side * 0.225, 1, 0) : V3(0, -0.25, 1)
    let projected = comb - seat.normal * dot(comb, seat.normal)
    let along = length_squared(projected) > 0.00001 ? normalize(projected)
      : normalize(cross(seat.normal, abs(seat.normal.x) < 0.8 ? V3(1, 0, 0) : V3(0, 1, 0)))
    let across = normalize(cross(along, seat.normal))
    let length: Float = ear ? 0.023 : head ? 0.043 : 0.067
    let width: Float = ear ? 0.013 : head ? 0.035 : 0.055
    let lift: Float = ear ? 0.0025 : 0.004
    let tint = ear ? V3(0.70, 0.46, 0.23) : parent.color
    func allowed(_ p: V3) -> Bool {
      p.y > 0.31 && exclusions.allSatisfy { $0.shape.value(at: p) > 0.008 }
        && (!ear || p.z > parent.pivot.z + 0.006)
    }
    func sample(_ u: Float, _ t: Float) throws -> V3 {
      let taper = 1 - 0.18 * t
      let target = seat.point + along * (t * length) + across * ((u - 0.5) * width * taper)
      let projected = try project(parent, toward: target)
      let elevation = -0.0015 + lift * sin(t * .pi) + t * 0.003
      return projected.point + projected.normal * elevation
    }
    // A coarse eligibility stencil is solely a deliberate exclusion mask.
    // Nonfinite/projection/accuracy failures are never converted into omissions.
    for t: Float in [0, 0.5, 1] {
      for u: Float in [0, 0.5, 1] {
        if !allowed(try sample(u, t)) { return nil }
      }
    }
    var fine: [V3] = []
    for row in 0...24 {
      for column in 0...8 {
        let p = try sample(Float(column) / 8, Float(row) / 24)
        try require(finite(p), "nonfinite undercoat patch")
        if !allowed(p) { return nil }
        fine.append(p)
      }
    }
    // Preserve source samples and refine only where the actual triangular
    // representation misses them. Both source coordinates of the worst sample
    // are retained; each unsuccessful pass adds at least one coordinate, so the
    // finite25x9 source grid bounds this process without an arbitrary retry cap.
    var rows = [0, 6, 12, 18, 24], columns = [0, 4, 8]
    func approximationError(_ rows: [Int], _ columns: [Int]) -> (Float, Int, Int) {
      var worst: Float = 0
      var worstRow = 0, worstColumn = 0
      for row in 0...24 {
        let yi = min(rows.count - 2, (rows.lastIndex { $0 <= row } ?? 0))
        let y = Float(row - rows[yi]) / Float(rows[yi + 1] - rows[yi])
        for column in 0...8 {
          let xi = min(columns.count - 2, (columns.lastIndex { $0 <= column } ?? 0))
          let x = Float(column - columns[xi]) / Float(columns[xi + 1] - columns[xi])
          let a: V3 = fine[rows[yi] * 9 + columns[xi]]
          let b: V3 = fine[rows[yi] * 9 + columns[xi + 1]]
          let c: V3 = fine[rows[yi + 1] * 9 + columns[xi]]
          let d: V3 = fine[rows[yi + 1] * 9 + columns[xi + 1]]
          let approximation: V3
          if x + y <= 1 {
            let edgeX: V3 = (b - a) * x
            let edgeY: V3 = (c - a) * y
            let alongX: V3 = a + edgeX
            approximation = alongX + edgeY
          } else {
            let edgeY: V3 = (b - d) * (Float(1) - y)
            let edgeX: V3 = (c - d) * (Float(1) - x)
            let alongY: V3 = d + edgeY
            approximation = alongY + edgeX
          }
          let distance = simd.length(fine[row * 9 + column] - approximation)
          if distance > worst { worst = distance; worstRow = row; worstColumn = column }
        }
      }
      return (worst, worstRow, worstColumn)
    }
    var deviation = approximationError(rows, columns)
    while deviation.0 > 0.001 {
      let addRow = !rows.contains(deviation.1), addColumn = !columns.contains(deviation.2)
      try require(addRow || addColumn, "retained source vertex has inconsistent approximation")
      if addRow { rows.append(deviation.1); rows.sort() }
      if addColumn { columns.append(deviation.2); columns.sort() }
      deviation = approximationError(rows, columns)
    }
    let error = deviation.0
    var mesh = Mesh()
    let phase = Float((number * 131 + 37011) % 4093)
    for row in rows {
      let t = Float(row) / 24
      for column in columns {
        let u = Float(column) / 8, index = row * 9 + column
        let du = fine[row * 9 + min(8, column + 1)] - fine[row * 9 + max(0, column - 1)]
        let dv = fine[min(24, row + 1) * 9 + column] - fine[max(0, row - 1) * 9 + column]
        let normal = cross(normalize(du), normalize(dv))
        try require(finite(normal) && length_squared(normal) > 1e-8, "degenerate undercoat patch frame")
        var vertex = Vertex(fine[index], normalize(normal), tint)
        // Native0237 calibration confirmed that unresolved .94 duty remains
        // near opaque. Constant duty therefore exposed every lifted polygon
        // side as a tile. Feather that representation across its source width;
        // the retained center column makes this tent exactly piecewise linear.
        // Positive epsilon keeps the existing groom sentinel active at edges
        // while remaining below the fragment coverage discard threshold.
        let transverseCoverage: Float = max(0.0001, 0.94 * (1 - abs(2 * u - 1)))
        vertex.groom = SIMD4(phase + u * width / 0.0008, t, transverseCoverage, 0.18)
        mesh.vertices.append(vertex)
      }
    }
    for row in 0..<(rows.count - 1) {
      for column in 0..<(columns.count - 1) {
        let a = UInt32(row * columns.count + column), b = a + 1, c = a + UInt32(columns.count)
        mesh.indices += [a, b, c, b, c + 1, c]
      }
    }
    try require(error <= 0.001, "\(parent.name) patch\(number) source deviation \(error * 1000) mm exceeds1mm")
    for vertex in mesh.vertices.prefix(columns.count) {
      try require(parent.shape.value(at: v3(vertex.position)) <= 0, "undercoat root edge left parent")
    }
    let guide = Guide(parent: parent.name, surface: seat.point, normal: seat.normal,
      points: [fine[4], fine[12 * 9 + 4], fine[24 * 9 + 4]], outerTriangle: seat.triangle,
      vertexStart: 0, vertexCount: mesh.vertices.count, rootWidth: columns.count,
      interiorVertex: rows.firstIndex(of: 6)! * columns.count + columns.firstIndex(of: 4)!,
      tipVertex: (rows.count - 1) * columns.count + columns.firstIndex(of: 4)!)
    return Patch(mesh: mesh, guide: guide, error: error)
  }

  static func compile(base: [Part]) throws -> Recipe {
    let byID = Dictionary(uniqueKeysWithValues: base.map { ($0.name, $0) })
    var parts = metadata(base: base), guides: [Guide] = []
    try require(parts.count == 4, "all four authored parent regions are required")
    let exclusions = base.filter { ["eyes", "muzzle", "nose"].contains(String($0.name.dropFirst(prefix.count))) }
    var bytes = 0, triangles = 0, exposed = 0, sampled = 0
    var error: Float = 0
    for region in parts.indices {
      let parentID = parts[region].parent!
      guard let parent = byID[parentID] else { throw RuntimeError.message("Sunhare coat: missing " + parentID) }
      let ear = region >= 2, head = region == 1
      var candidates = try seats(parent, ear: ear, head: head)
      var nearest = [Float](repeating: .greatestFiniteMagnitude, count: candidates.count)
      var selected: [Mesh] = []
      var regionVertices = 0
      var chosen = candidates.indices.max { candidates[$0].point.y < candidates[$1].point.y }
      while selected.count < regionCounts[region], let index = chosen {
        let seat = candidates.remove(at: index)
        nearest.remove(at: index)
        if let result = try patch(parent, seat: seat, ear: ear, head: head,
          number: region * 409 + selected.count, exclusions: exclusions) {
          var guide = result.guide
          guide.vertexStart = regionVertices
          regionVertices += result.mesh.vertices.count
          selected.append(result.mesh); guides.append(guide); error = max(error, result.error)
          for i in candidates.indices { nearest[i] = min(nearest[i], length_squared(candidates[i].point - seat.point)) }
        }
        chosen = candidates.indices.max { nearest[$0] < nearest[$1] }
      }
      try require(selected.count == regionCounts[region], "insufficient safely covered surface on " + parentID)
      let mesh = ParametricMesh.joined(selected)
      bytes += mesh.vertices.count * MemoryLayout<Vertex>.stride + mesh.indices.count * MemoryLayout<UInt32>.stride
      triangles += mesh.indices.count / 3
      for vertex in mesh.vertices {
        sampled += 1
        if parent.shape.value(at: v3(vertex.position)) > 0.001 { exposed += 1 }
      }
      parts[region].proceduralMesh = mesh
    }
    try require(triangles <= maximumTriangles && bytes <= maximumMeshBytes, "undercoat exceeded triangle/storage budget")
    let fraction = Float(exposed) / Float(max(1, sampled))
    try require(fraction > 0.55, "undercoat is mostly buried")
    return Recipe(parts: parts, guides: guides, meshBytes: bytes,
      maximumReductionError: error, exposedVertexFraction: fraction)
  }
}
