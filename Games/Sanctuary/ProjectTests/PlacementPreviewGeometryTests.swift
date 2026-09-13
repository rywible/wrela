import FieldCompiler
import FieldCore
import FieldEngine
import SanctuaryContent
import XCTest
import simd
@testable import SanctuaryProject

/// Checks source-space feature coverage through the production extractor. No
/// renderer, saved placement, alternate construction shape or pixel baseline.
final class PlacementPreviewGeometryTests: XCTestCase {
  private func xyz(_ p: SIMD4<Float>) -> V3 { V3(p.x, p.y, p.z) }

  private func points(_ batches: [SceneBatch]) -> [V3] {
    batches.flatMap { batch in
      batch.instances.flatMap { instance in batch.mesh.vertices.map { xyz(instance.model * $0.position) } }
    }
  }

  private func bounds(_ points: [V3]) -> Bounds {
    Bounds(points.reduce(V3(repeating: Float.greatestFiniteMagnitude)) { simd_min($0, $1) },
      points.reduce(V3(repeating: -Float.greatestFiniteMagnitude)) { simd_max($0, $1) })
  }

  private func distance(_ point: V3, to mesh: Mesh) -> Float {
    var nearest = Float.greatestFiniteMagnitude
    for index in stride(from: 0, to: mesh.indices.count, by: 3) {
      let p = (0..<3).map { xyz(mesh.vertices[Int(mesh.indices[index + $0])].position) }
      let bary = CostumeCompiler.closestBarycentric(point, p[0], p[1], p[2])
      nearest = min(nearest, simd_distance(point, p[0] * bary.x + p[1] * bary.y + p[2] * bary.z))
    }
    return nearest
  }

  private func assertCovered(_ a: V3, _ b: V3, by mesh: Mesh,
    file: StaticString = #filePath, line: UInt = #line) {
    for fraction: Float in [0, 0.25, 0.5, 0.75, 1] {
      let p = mix(a, b, t: fraction)
      XCTAssertLessThan(distance(p, to: mesh), 0.045,
        "Source outline missing near \(p)", file: file, line: line)
    }
  }

  private func mix(_ a: V3, _ b: V3, t: Float) -> V3 { a + (b - a) * t }

  func testCabinKeepsFlatWallOutlinesRoofRidgeAndOpenDoorway() throws {
    let source = try LivingWorldPresentation.constructionBatches(for: .cabin)
    let mesh = try SanctuaryPlacementPresentation.contourBatch(for: source, name: "Cabin contour regression").mesh
    let walls = try XCTUnwrap(source.first { $0.name == "Construction cabin timber walls" })
    let wallPoints = points([walls]), wallBounds = bounds(wallPoints)
    // Read the long planar back-wall edges from the real source. Thin siding
    // finishes sit within this band; only the structural wall reaches both X extremes.
    let back = wallPoints.filter { $0.z >= wallBounds.max.z - 0.03 }
    let backBounds = bounds(back)
    var wallBottom: Float = 0, wallTop: Float = 0
    for x in [backBounds.min.x, backBounds.max.x] {
      let edge = back.filter { abs($0.x - x) < 0.001 }
      let a = try XCTUnwrap(edge.min { $0.y < $1.y })
      let b = try XCTUnwrap(edge.max { $0.y < $1.y })
      wallBottom = a.y; wallTop = b.y
      XCTAssertGreaterThan(b.y - a.y, 2, "A real full-height wall edge must be exercised")
      assertCovered(a, b, by: mesh)
    }
    let roof = try XCTUnwrap(source.first { $0.name == "Construction cabin roof" })
    let roofPoints = points([roof]), roofBounds = bounds(roofPoints)
    let ridge = roofPoints.filter { abs($0.y - roofBounds.max.y) < 0.001 }
    assertCovered(try XCTUnwrap(ridge.min { $0.z < $1.z }),
      try XCTUnwrap(ridge.max { $0.z < $1.z }), by: mesh)
    let doorway = V3((backBounds.min.x + backBounds.max.x) * 0.5,
      (wallBottom + wallTop) * 0.5, wallBounds.min.z)
    XCTAssertGreaterThan(distance(doorway, to: mesh), 0.4,
      "The preview must leave the central doorway open")
  }

  func testThinPlatformsKeepBothSidesAndEveryRegisteredPreviewStaysWithinBudget() throws {
    let platforms: [PersonalConstruction.Primitive] = [.deck, .bridge]
    for primitive in platforms {
      let source = try LivingWorldPresentation.constructionBatches(for: primitive)
      let profile = primitive == .bridge
        ? SanctuaryWalkableConstructionProfile.bridge : SanctuaryWalkableConstructionProfile.deck
      let platformName = primitive == .bridge ? "Construction bridge deck" : "Construction deck platform"
      let platform = try XCTUnwrap(source.first { $0.name == platformName })
      // Compare actual persisted collision heights with the compiled platform
      // surface, including the allowed scale endpoints and rotated placements.
      // Interior probes avoid the deck's corner posts and extraction edge bevels.
      for scale: Float in [0.5, 1, 3] {
        for yaw: Float in [0, .pi / 2] {
          var construction = PersonalConstruction()
          let base = PersonalConstruction.Location(x: 3.51, y: 1.7125, z: 21.13)
          try construction.apply(.place(primitive, at: base, yawRadians: yaw, scale: scale),
            expectedRevision: construction.revision)
          let support = try XCTUnwrap(construction.collisionFacts.first { $0.kind == .walkable })
          for fraction: Float in [-0.7, 0, 0.7] {
            let local = V3(profile.platform.halfExtents.x * fraction,
              (support.top - base.y) / scale, 0)
            let world = PersonalConstruction.Location(
              x: base.x + local.x * cos(yaw) * scale, y: support.top,
              z: base.z - local.x * sin(yaw) * scale)
            XCTAssertTrue(support.contains(world))
            XCTAssertLessThan(distance(local, to: platform.mesh), 0.015,
              "\(primitive) support must meet its extracted surface at scale \(scale), yaw \(yaw)")
          }
        }
      }
      let mesh = try SanctuaryPlacementPresentation.contourBatch(for: source, name: "Platform contour regression").mesh
      let footprint = bounds(points(source))
      // Project source slab perimeter samples to XZ. This allows either existing
      // top/bottom source edge to represent the slab, but cannot pass with only
      // one side, the interior supports, or a lone center line.
      let corners = [SIMD2(footprint.min.x, footprint.min.z), SIMD2(footprint.max.x, footprint.min.z),
        SIMD2(footprint.max.x, footprint.max.z), SIMD2(footprint.min.x, footprint.max.z)]
      for side in 0..<4 {
        for fraction: Float in [0.1, 0.3, 0.5, 0.7, 0.9] {
          let p = corners[side] + (corners[(side + 1) % 4] - corners[side]) * fraction
          var nearest = Float.greatestFiniteMagnitude
          for index in stride(from: 0, to: mesh.indices.count, by: 3) {
            let triangle = (0..<3).map { offset -> SIMD2<Float> in
              let v = mesh.vertices[Int(mesh.indices[index + offset])].position
              return SIMD2(v.x, v.z)
            }
            for edge in 0..<3 {
              let a = triangle[edge], delta = triangle[(edge + 1) % 3] - a
              let denominator = length_squared(delta)
              let t: Float = denominator > 0 ? max(0, min(1, dot(p - a, delta) / denominator)) : 0
              nearest = min(nearest, simd_distance(p, a + delta * t))
            }
          }
          XCTAssertLessThan(nearest, 0.045, "\(primitive) lost perimeter near \(p)")
        }
      }
    }
    // Kind 4 is cylindrical living bark, including angular grain and wind bend.
    // Planar timber must stay rigid; the authored stone path remains stone.
    let constructionMaterials: [PersonalConstruction.Primitive: Float] = [
      .bridge: 7, .deck: 7, .bench: 7, .path: 5,
    ]
    for primitive in PersonalConstruction.Primitive.allCases {
      if let material = constructionMaterials[primitive] {
        let source = try LivingWorldPresentation.constructionBatches(for: primitive)
        XCTAssertTrue(source.flatMap(\.instances).allSatisfy { $0.tint.w == material },
          "\(primitive) must use its rigid timber/stone response, not cylindrical bark")
      }
      let generator = try XCTUnwrap(SanctuaryPlacementPresentation.generators.first {
        $0.id == "construction-preview-\(primitive.rawValue)"
      })
      let compile = try XCTUnwrap(generator.compile)
      for valid: Float in [0, 1] {
        let batches = try compile(AssetSource(id: generator.id, name: generator.name,
          generator: generator.id, parameters: ["valid": valid, "yaw": 0, "scale": 1]))
        XCTAssertEqual(batches.count, 2)
        XCTAssertLessThanOrEqual(batches.reduce(0) { $0 + $1.mesh.indices.count / 3 }, 5_940)
        XCTAssertTrue(batches.allSatisfy { !$0.mesh.indices.isEmpty })
        XCTAssertTrue(points(batches).allSatisfy { $0.x.isFinite && $0.y.isFinite && $0.z.isFinite })
      }
    }
  }

  func testActualBenchKeepsSeatAndBackOutlinesWhenMirroredOrRotatedBeforeExtraction() throws {
    let source = try LivingWorldPresentation.constructionBatches(for: .bench)
    let originalPoints = points(source), originalBounds = bounds(originalPoints)
    let topFace = originalPoints.filter { $0.y >= originalBounds.max.y - 0.002 }
    let topFront = try XCTUnwrap(topFace.map(\.z).min())
    let top = topFace.filter { $0.z <= topFront + 0.002 }
    let front = originalPoints.filter { $0.z <= originalBounds.min.z + 0.002 }
    let frontTop = try XCTUnwrap(front.map(\.y).max())
    let seat = front.filter { $0.y >= frontTop - 0.002 }
    let features = [(try XCTUnwrap(top.min { $0.x < $1.x }), try XCTUnwrap(top.max { $0.x < $1.x })),
      (try XCTUnwrap(seat.min { $0.x < $1.x }), try XCTUnwrap(seat.max { $0.x < $1.x }))]
    XCTAssertTrue(features.allSatisfy { simd_distance($0.0, $0.1) > 1.5 })
    let turn = simd_float4x4(simd_quatf(angle: .pi / 4, axis: V3(0, 1, 0)))
    let mirror = simd_float4x4(diagonal: SIMD4(-1, 1, 1, 1))
    for matrix in [matrix_identity_float4x4, turn, mirror, turn * mirror] {
      let transformed = source.map { batch -> SceneBatch in
        var result = batch
        result.instances = batch.instances.map { instance in
          var value = instance; value.model = matrix * value.model; return value
        }
        return result
      }
      let mesh = try SanctuaryPlacementPresentation.contourBatch(for: transformed,
        name: "Transformed bench contour regression").mesh
      XCTAssertLessThanOrEqual(mesh.indices.count / 3, 5_760)
      for (a, b) in features { assertCovered(xyz(matrix * SIMD4(a, 1)), xyz(matrix * SIMD4(b, 1)), by: mesh) }
    }
  }
}
