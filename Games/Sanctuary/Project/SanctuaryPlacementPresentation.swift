import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import simd

/// A transient survey of the actual construction source, not an alternate placement model.
/// Connected surface contours keep the destination visible without an opaque filled ghost.
final class SanctuaryPlacementPresentation {
  private static let validColor = V3(0.52, 0.68, 0.46)
  private static let invalidColor = V3(0.76, 0.43, 0.30)
  private static let maximumContourSegments = 480
  private static let supportJoints = (0..<5).map { "Placement support \($0)" }

  private struct Recipe {
    let contours: SceneBatch
    let validMarks: SceneBatch
    let invalidMarks: SceneBatch
    let corners: [V3]
  }

  private struct Uploaded {
    let contours: GPUBatch
    let validMarks: GPUBatch
    let invalidMarks: GPUBatch
    let corners: [V3]
  }

  private let batches: [PersonalConstruction.Primitive: Uploaded]
  private var cachedDraft: SanctuaryPlacementIntent.Draft?
  private var cachedSupportRevision: UInt64?
  private var supportPalette = [simd_float4x4](repeating: matrix_identity_float4x4, count: 5)
  private(set) var supportQueriesLastFrame = 0

  init(graphics: MetalRenderer) throws {
    batches = try Dictionary(uniqueKeysWithValues: PersonalConstruction.Primitive.allCases.map { primitive in
      let recipe = try Self.recipe(for: primitive)
      return (primitive, Uploaded(contours: graphics.upload(recipe.contours),
        validMarks: graphics.upload(recipe.validMarks), invalidMarks: graphics.upload(recipe.invalidMarks),
        corners: recipe.corners))
    })
  }

  func reset() {
    cachedDraft = nil
    cachedSupportRevision = nil
    supportQueriesLastFrame = 0
    supportPalette = Array(repeating: matrix_identity_float4x4, count: 5)
  }

  func items(
    draft: SanctuaryPlacementIntent.Draft?, valid: Bool, supportRevision: UInt64,
    terrainHeight: (Float, Float) -> Float
  ) -> [RenderItem] {
    supportQueriesLastFrame = 0
    guard let draft, let geometry = batches[draft.primitive],
      [draft.location.x, draft.location.y, draft.location.z, draft.yawRadians, draft.scale].allSatisfy(\.isFinite),
      abs(draft.location.x) <= 20_000, abs(draft.location.y) <= 1_000, abs(draft.location.z) <= 20_000,
      (0.1...4).contains(draft.scale)
    else { reset(); return [] }
    let root = Self.placementTransform(location: draft.location, yaw: draft.yawRadians, scale: draft.scale)
    if cachedDraft != draft || cachedSupportRevision != supportRevision {
      supportPalette = Array(repeating: matrix_identity_float4x4, count: 5)
      for index in geometry.corners.indices {
        let point = root * SIMD4(geometry.corners[index], 1)
        let height = terrainHeight(point.x, point.z)
        supportQueriesLastFrame += 1
        // An unavailable/extreme support cannot create an unbounded presentation transform.
        // Keeping that marker at the intended base does not confer placement validity.
        if height.isFinite, abs(height - draft.location.y) <= 128 {
          supportPalette[index].columns.3.y = (height - draft.location.y) / draft.scale
        }
      }
      cachedDraft = draft
      cachedSupportRevision = supportRevision
    }
    let color = valid ? Self.validColor : Self.invalidColor
    var result: [RenderItem] = []
    for batch in [geometry.contours, valid ? geometry.validMarks : geometry.invalidMarks] {
      var instance = batch.sourceInstances[0]
      instance.model = root * instance.model
      instance.tint = SIMD4(color, 7)
      var item = RenderItem(batch: batch, instance: instance, castsShadow: false)
      if !batch.skinJoints.isEmpty { item.skinPalette = supportPalette }
      result.append(item)
    }
    return result
  }

  private static func placementTransform(
    location: PersonalConstruction.Location, yaw: Float, scale: Float
  ) -> simd_float4x4 {
    transform(V3(location.x, location.y, location.z), V3(repeating: scale), yaw)
  }

  private static let recipes: Result<[PersonalConstruction.Primitive: Recipe], Error> = Result {
    try Dictionary(uniqueKeysWithValues: PersonalConstruction.Primitive.allCases.map { primitive in
      (primitive, try makeRecipe(primitive))
    })
  }

  private static func recipe(for primitive: PersonalConstruction.Primitive) throws -> Recipe {
    guard let recipe = try recipes.get()[primitive] else {
      throw RuntimeError.message("Missing construction preview source")
    }
    return recipe
  }

  private struct VertexKey: Hashable {
    let x: Int
    let y: Int
    let z: Int

    init(_ p: V3) {
      x = Int((p.x * 100_000).rounded())
      y = Int((p.y * 100_000).rounded())
      z = Int((p.z * 100_000).rounded())
    }
  }

  private struct EdgeKey: Hashable {
    let a: Int
    let b: Int

    init(_ first: Int, _ second: Int) {
      a = min(first, second)
      b = max(first, second)
    }
  }

  private struct SurfaceEdge {
    let key: EdgeKey
    var normals: [V3]
  }

  private struct Contour {
    let points: [V3]
    let span: Float
    let length: Float

    var strokes: [Stroke] {
      (1..<points.count).map { Stroke(a: points[$0 - 1], b: points[$0]) }
    }
  }

  private struct Stroke {
    let a: V3
    let b: V3
  }

  private static func makeRecipe(_ primitive: PersonalConstruction.Primitive) throws -> Recipe {
    let source = try LivingWorldPresentation.constructionBatches(for: primitive)
    let contours = try contourBatch(for: source, name: "Placement \(primitive.rawValue) surface contours")
    let half = primitive.halfExtents
    let corners = [V3(-half.x, 0, -half.z), V3(half.x, 0, -half.z),
      V3(-half.x, 0, half.z), V3(half.x, 0, half.z)]
    return Recipe(contours: contours, validMarks: marks(primitive, corners: corners, valid: true),
      invalidMarks: marks(primitive, corners: corners, valid: false), corners: corners)
  }

  /// Production source-to-preview compilation, independent of a renderer or draft.
  /// Geometry regressions exercise this same entry with the actual construction meshes.
  static func contourBatch(for source: [SceneBatch], name: String) throws -> SceneBatch {
    var lo = V3(repeating: Float.greatestFiniteMagnitude), hi = -lo
    for batch in source {
      for instance in batch.instances {
        for vertex in batch.mesh.vertices {
          let p = instance.model * vertex.position
          let point = V3(p.x, p.y, p.z)
          lo = simd_min(lo, point)
          hi = simd_max(hi, point)
        }
      }
    }
    guard [lo.x, lo.y, lo.z, hi.x, hi.y, hi.z].allSatisfy(\.isFinite) else {
      throw RuntimeError.message("Construction preview needs finite source bounds")
    }
    let extent = hi - lo
    let span = max(extent.x, max(extent.y, extent.z))
    // Scale stroke width in metres for ordinary aiming distance. The surface stays
    // open, including the cabin doorway and the ground beneath furniture/platforms.
    let radius = min(0.016, max(0.011, span * 0.003))
    let tolerance = max(0.005, span * 0.0015)
    var paths: [Contour] = []
    for batch in source {
      for instance in batch.instances {
        paths += surfaceContours(batch.mesh, model: instance.model,
          tolerance: tolerance, minimumThickness: radius * 1.6)
      }
    }
    // Preserve complete feature chains in descending physical size. Never spend the
    // budget by punching regular holes into a source contour. The stable tie-break
    // keeps recipe compilation deterministic even when equal-size parts repeat.
    let ordered = paths.enumerated().sorted { lhs, rhs in
      if lhs.element.span != rhs.element.span { return lhs.element.span > rhs.element.span }
      if lhs.element.length != rhs.element.length { return lhs.element.length > rhs.element.length }
      return lhs.offset < rhs.offset
    }
    // Wall thickness and trim produce several almost coincident interior rails.
    // Keep their outward source edge, rather than giving every nested face equal
    // visual weight. Thin objects get a proportionally smaller comparison band.
    let clearance = max(radius * 2, min(0.35, min(extent.x, min(extent.y, extent.z)) * 0.10))
    let center = (lo + hi) * 0.5
    var retained: [Contour] = []
    var retainedCount = 0
    for entry in ordered {
      let candidate = entry.element
      let candidateStrokes = candidate.strokes
      let existing = retained.flatMap(\.strokes)
      guard !covered(candidateStrokes, by: existing, center: center, clearance: clearance) else { continue }
      // A later, slightly shorter outer trim edge can replace a previously chosen
      // inner rail. Remove complete contours only, so curved caps never get holes.
      let replaced = retained.indices.filter {
        covered(retained[$0].strokes, by: candidateStrokes, center: center, clearance: clearance)
      }
      let removedCount = replaced.reduce(0) { $0 + retained[$1].points.count - 1 }
      let newCount = retainedCount - removedCount + candidateStrokes.count
      guard newCount <= maximumContourSegments else { continue }
      for index in replaced.reversed() { retained.remove(at: index) }
      retained.append(candidate)
      retainedCount = newCount
    }
    var bars: [Mesh] = []
    var drawn = Set<[VertexKey]>()
    for contour in retained {
      let points = contour.points
      var additions: [(V3, V3, [VertexKey])] = []
      for index in 1..<points.count {
        let a = points[index - 1], b = points[index]
        let key = [VertexKey(a), VertexKey(b)]
        guard !drawn.contains(key), !drawn.contains(Array(key.reversed())) else { continue }
        additions.append((a, b, key))
      }
      guard bars.count + additions.count <= maximumContourSegments else { continue }
      for (a, b, key) in additions {
        drawn.insert(key)
        bars.append(bar(from: a, to: b, radius: radius))
      }
    }
    return SceneBatch(name: name,
      mesh: ParametricMesh.joined(bars), instances: [Instance(tint: validColor, kind: 7)], roughness: 0.82)
  }

  /// Suppress only a fully represented inner feature. All endpoints remain on
  /// original source facets; neither the collision footprint nor silhouette is
  /// replaced with a new box. Opposite sides of the object cannot cover each other.
  private static func covered(
    _ candidate: [Stroke], by existing: [Stroke], center: V3, clearance: Float
  ) -> Bool {
    guard !candidate.isEmpty, !existing.isEmpty else { return false }
    return candidate.allSatisfy { line in
      existing.contains { other in
        let a = VertexKey(line.a), b = VertexKey(line.b)
        if (a == VertexKey(other.a) && b == VertexKey(other.b))
          || (a == VertexKey(other.b) && b == VertexKey(other.a)) { return true }
        let delta = line.b - line.a, otherDelta = other.b - other.a
        let span = length(delta), otherSpan = length(otherDelta)
        // Small cap/return edges express cross-section; proximity alone must not
        // erase them. Long near-parallel duplicate rails are the demonstrated noise.
        guard span >= clearance * 2, otherSpan >= span * 0.9 else { return false }
        let direction = otherDelta / otherSpan
        guard abs(dot(delta / span, direction)) >= 0.999 else { return false }
        for point in [line.a, line.b] {
          let along = dot(point - other.a, direction)
          guard along >= -clearance * 0.25, along <= otherSpan + clearance * 0.25,
            length_squared(point - (other.a + direction * along)) <= clearance * clearance
          else { return false }
        }
        let midpoint = (line.a + line.b) * 0.5 - center
        let otherMidpoint = (other.a + other.b) * 0.5 - center
        let perpendicular = midpoint - direction * dot(midpoint, direction)
        let otherPerpendicular = otherMidpoint - direction * dot(otherMidpoint, direction)
        // An accepted rail must be at least as far outward in every transverse
        // coordinate. This protects roof/eave extremes and both sides of thin rails.
        for axis in 0..<3 {
          guard abs(perpendicular[axis]) <= abs(otherPerpendicular[axis]) + 0.001,
            perpendicular[axis] * otherPerpendicular[axis] >= -0.000001
          else { return false }
        }
        return true
      }
    }
  }

  /// Weld the compiled surface, then trace its creases and three orthographic
  /// silhouette families. This works for both field meshes and authored prisms;
  /// no construction dimensions or placement rules are restated here.
  private static func surfaceContours(
    _ mesh: Mesh, model: simd_float4x4, tolerance: Float, minimumThickness: Float
  ) -> [Contour] {
    var points: [V3] = []
    var welded: [VertexKey: Int] = [:]
    var remap: [Int] = []
    for vertex in mesh.vertices {
      let transformed = model * vertex.position
      let point = V3(transformed.x, transformed.y, transformed.z)
      let key = VertexKey(point)
      if let index = welded[key] { remap.append(index) }
      else {
        welded[key] = points.count
        remap.append(points.count)
        points.append(point)
      }
    }
    var parent = Array(points.indices)
    func root(_ index: Int) -> Int {
      var result = index
      while parent[result] != result { result = parent[result] }
      var cursor = index
      while parent[cursor] != cursor {
        let next = parent[cursor]
        parent[cursor] = result
        cursor = next
      }
      return result
    }
    var edges: [SurfaceEdge] = []
    var edgeIndices: [EdgeKey: Int] = [:]
    for index in stride(from: 0, to: mesh.indices.count, by: 3) {
      let ids = (0..<3).map { remap[Int(mesh.indices[index + $0])] }
      let normal = cross(points[ids[1]] - points[ids[0]], points[ids[2]] - points[ids[0]])
      guard length_squared(normal) > 0.000000000001 else { continue }
      let unitNormal = normalize(normal)
      for corner in 0..<3 {
        let key = EdgeKey(ids[corner], ids[(corner + 1) % 3])
        let firstRoot = root(key.a), secondRoot = root(key.b)
        if firstRoot != secondRoot { parent[secondRoot] = firstRoot }
        if let existing = edgeIndices[key] { edges[existing].normals.append(unitNormal) }
        else {
          edgeIndices[key] = edges.count
          edges.append(SurfaceEdge(key: key, normals: [unitNormal]))
        }
      }
    }
    var bounds: [Int: (V3, V3)] = [:]
    for index in points.indices {
      let component = root(index), p = points[index]
      if let (lo, hi) = bounds[component] { bounds[component] = (simd_min(lo, p), simd_max(hi, p)) }
      else { bounds[component] = (p, p) }
    }
    var selected: [EdgeKey] = []
    for edge in edges {
      guard let (lo, hi) = bounds[root(edge.key.a)] else { continue }
      let size = hi - lo
      // Suppress separate veneer/plank-seam shells below stroke thickness. The
      // structural wall/floor remains; tiny decorative boxes would double its ink.
      guard min(size.x, min(size.y, size.z)) >= minimumThickness else { continue }
      let normals = edge.normals
      var feature = normals.count == 1
      if let first = normals.first {
        feature = feature || normals.dropFirst().contains { dot(first, $0) < 0.70 }
      }
      for axis in 0..<3 where !feature {
        // A small grazing bias avoids unstable coplanar tessellation edges. The
        // resulting chain stays on the actual surface, close to its silhouette.
        let front = normals.contains { $0[axis] > 0.06 }
        let back = normals.contains { $0[axis] <= 0.06 }
        feature = front && back
      }
      if feature { selected.append(edge.key) }
    }
    var adjacency = [[Int]](repeating: [], count: points.count)
    for (index, edge) in selected.enumerated() {
      adjacency[edge.a].append(index)
      adjacency[edge.b].append(index)
    }
    var visited = [Bool](repeating: false, count: selected.count)
    var result: [Contour] = []
    func trace(_ firstEdge: Int, from start: Int) {
      var chain = [points[start]]
      var vertex = start, current = firstEdge
      while !visited[current] {
        visited[current] = true
        let edge = selected[current]
        vertex = edge.a == vertex ? edge.b : edge.a
        chain.append(points[vertex])
        guard vertex != start, adjacency[vertex].count == 2,
          let next = adjacency[vertex].first(where: { !visited[$0] }) else { break }
        current = next
      }
      let simplified = simplify(chain, tolerance: tolerance)
      guard simplified.count > 1 else { return }
      var lo = simplified[0], hi = lo, total: Float = 0
      for index in simplified.indices {
        lo = simd_min(lo, simplified[index]); hi = simd_max(hi, simplified[index])
        if index > 0 { total += distance(simplified[index - 1], simplified[index]) }
      }
      let size = hi - lo, span = max(size.x, max(size.y, size.z))
      guard span >= minimumThickness * 1.5 else { return }
      result.append(Contour(points: simplified, span: span, length: total))
    }
    // Junctions delimit complete features; remaining degree-two components are
    // closed loops. Edge insertion order follows the deterministic source mesh.
    for vertex in adjacency.indices where adjacency[vertex].count != 2 {
      for edge in adjacency[vertex] where !visited[edge] { trace(edge, from: vertex) }
    }
    for index in selected.indices where !visited[index] { trace(index, from: selected[index].a) }
    return result
  }

  /// Bound deviation from the source chain, including closed loops. Removing
  /// collinear tessellation points turns a hundred tiny cuts into one full rail.
  private static func simplify(_ points: [V3], tolerance: Float) -> [V3] {
    guard points.count > 2 else { return points }
    var keep = [Bool](repeating: false, count: points.count)
    keep[0] = true; keep[points.count - 1] = true
    var pending = [(0, points.count - 1)]
    while let (first, last) = pending.popLast() {
      guard last > first + 1 else { continue }
      let delta = points[last] - points[first]
      let squaredLength = length_squared(delta)
      var farthest = first, error = tolerance * tolerance
      for index in (first + 1)..<last {
        let fraction = squaredLength > 0.000000000001
          ? clamp(dot(points[index] - points[first], delta) / squaredLength, 0, 1) : 0
        let candidate = distance_squared(points[index], points[first] + delta * fraction)
        if candidate > error { error = candidate; farthest = index }
      }
      if farthest != first {
        keep[farthest] = true
        pending.append((first, farthest)); pending.append((farthest, last))
      }
    }
    return points.indices.filter { keep[$0] }.map { points[$0] }
  }

  private static func marks(
    _ primitive: PersonalConstruction.Primitive, corners: [V3], valid: Bool
  ) -> SceneBatch {
    var mesh = Mesh()
    var weights: [SkinWeight] = []
    func add(_ piece: Mesh, joint: UInt32) {
      let offset = UInt32(mesh.vertices.count)
      mesh.vertices += piece.vertices
      mesh.indices += piece.indices.map { $0 + offset }
      weights += piece.vertices.map { _ in SkinWeight(joint) }
    }
    let half = primitive.halfExtents
    let length = min(0.35, max(0.08, min(half.x, half.z) * 0.4))
    for (index, corner) in corners.enumerated() {
      let p = corner + V3(0, 0.045, 0)
      add(bar(from: p, to: p + V3(0, 0.24, 0), radius: 0.016), joint: UInt32(index))
      add(bar(from: p, to: p + V3(corner.x < 0 ? length : -length, 0, 0), radius: 0.012), joint: UInt32(index))
      add(bar(from: p, to: p + V3(0, 0, corner.z < 0 ? length : -length), radius: 0.012), joint: UInt32(index))
    }
    let span = min(0.3, max(0.1, half.x * 0.25))
    let center = V3(0, 0.065, -half.z - span * 0.55)
    // The front marker shares the draft base. A cross supplements the rejected tint;
    // an arrow identifies forward/door orientation without concealing the destination.
    if valid {
      add(bar(from: center + V3(0, 0, span), to: center - V3(0, 0, span), radius: 0.013), joint: 4)
      for side: Float in [-1, 1] {
        add(bar(from: center - V3(0, 0, span), to: center + V3(side * span * 0.65, 0, 0), radius: 0.013), joint: 4)
      }
    } else {
      for side: Float in [-1, 1] {
        add(bar(from: center + V3(-span, 0, side * span), to: center + V3(span, 0, -side * span), radius: 0.016), joint: 4)
      }
    }
    var batch = SceneBatch(name: "Placement \(primitive.rawValue) \(valid ? "ready" : "rejected") marks",
      mesh: mesh, instances: [Instance(tint: valid ? validColor : invalidColor, kind: 7)], roughness: 0.88)
    batch.skinJoints = supportJoints
    batch.skinWeights = weights
    return batch
  }

  /// Twelve triangles per survey bar, compiled once. Width is authored in metres.
  private static func bar(from a: V3, to b: V3, radius: Float) -> Mesh {
    let delta = b - a
    let rotation = simd_quatf(from: V3(0, 1, 0), to: normalize(delta))
    var mesh = SanctuaryProceduralCraft.box(.zero, V3(radius, length(delta) * 0.5, radius), color: V3(repeating: 1))
    for index in mesh.vertices.indices {
      let vertex = mesh.vertices[index]
      let p = rotation.act(V3(vertex.position.x, vertex.position.y, vertex.position.z)) + (a + b) * 0.5
      let n = rotation.act(V3(vertex.normal.x, vertex.normal.y, vertex.normal.z))
      mesh.vertices[index] = Vertex(p, n, V3(repeating: 1))
    }
    return mesh
  }

  static var generators: [AssetGenerator] {
    PersonalConstruction.Primitive.allCases.map { primitive in
      AssetGenerator(id: "construction-preview-\(primitive.rawValue)",
        name: "Placement preview · \(primitive.rawValue.capitalized)", controls: [
          ScalarControl("valid", "Ready to place", 1, 0...1),
          ScalarControl("yaw", "Facing · degrees", 0, -180...180),
          ScalarControl("scale", "Uniform scale", 1, 0.5...3),
        ], compile: { source in
          let recipe = try recipe(for: primitive)
          let valid = (source.parameters["valid"] ?? 1) >= 0.5
          let root = placementTransform(location: .init(x: 0, y: 0, z: 0),
            yaw: (source.parameters["yaw"] ?? 0) * .pi / 180, scale: source.parameters["scale"] ?? 1)
          return [recipe.contours, valid ? recipe.validMarks : recipe.invalidMarks].map { original in
            var batch = original
            batch.instances = [Instance(tint: valid ? validColor : invalidColor, kind: 7)]
            batch.instances[0].model = root
            // Flat studio support is the identity deformation of the production marker mesh.
            batch.skinJoints = []; batch.skinWeights = []
            return batch
          }
        })
    }
  }
}
