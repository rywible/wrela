import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import XCTest
import simd
@testable import SanctuaryProject

final class GardenReedGeometryTests: XCTestCase {
  // Frozen four-capsule production recipe before the appearance candidate. Keep this
  // baseline available after eventual integration rather than silently measuring the new mesh.
  private static var originalShape: Shape {
    Shape.capsule(V3(-0.12, 0, 0.05), V3(-0.11, 0.82, 0.04), 0.018)
      .joined(.capsule(V3(0.09, 0, -0.08), V3(0.10, 1.02, -0.06), 0.020))
      .joined(.capsule(V3(0.19, 0, 0.12), V3(0.18, 0.69, 0.11), 0.017))
      .joined(.capsule(V3(-0.04, 0, -0.16), V3(-0.02, 0.90, -0.15), 0.016))
  }
  func testOriginalReedMeshReceiptRemainsFrozen() throws {
    let batch = try LivingWorldPresentation.gardenReedBaselineBatch()
    let baseline = try Mesher.compile(Self.originalShape, resolution: 24)
    XCTAssertEqual(batch.mesh.indices, baseline.indices)
    XCTAssertEqual(batch.mesh.vertices.map(\.position), baseline.vertices.map(\.position))
    XCTAssertEqual(batch.instances.count, 1)
    XCTAssertEqual(batch.instances[0].tint.w, 8)
    XCTAssertFalse(batch.mesh.indices.isEmpty)
    let points = batch.mesh.vertices.map { V3($0.position.x, $0.position.y, $0.position.z) }
    XCTAssertTrue(points.allSatisfy { $0.x.isFinite && $0.y.isFinite && $0.z.isFinite })
    let low = points.reduce(V3(repeating: Float.greatestFiniteMagnitude), simd_min)
    let high = points.reduce(V3(repeating: -Float.greatestFiniteMagnitude), simd_max)
    let receipt: [String: Any] = [
      "vertices": batch.mesh.vertices.count, "triangles": batch.mesh.indices.count / 3,
      "sourceBytes": batch.mesh.vertices.count * MemoryLayout<Vertex>.stride + batch.mesh.indices.count * 4,
      "min": [low.x, low.y, low.z], "max": [high.x, high.y, high.z],
      "material": batch.instances[0].tint.w, "roughness": batch.roughness]
    let json = try JSONSerialization.data(withJSONObject: receipt, options: [.sortedKeys])
    print("GardenReedGeometry original receipt: " + String(decoding: json, as: UTF8.self))
  }
  func testProductionUsesTheExactReviewedMeshAndMaterial() throws {
    let production = try LivingWorldPresentation.gardenReedBatch()
    let reviewed = GardenReedStudy.batch()
    XCTAssertEqual(production.mesh.indices, reviewed.mesh.indices)
    XCTAssertEqual(production.mesh.vertices.map(\.position), reviewed.mesh.vertices.map(\.position))
    XCTAssertEqual(production.mesh.vertices.map(\.normal), reviewed.mesh.vertices.map(\.normal))
    XCTAssertEqual(production.mesh.vertices.map(\.color), reviewed.mesh.vertices.map(\.color))
    XCTAssertEqual(production.instances.count, 1)
    for column in 0..<4 {
      XCTAssertEqual(production.instances[0].model[column], reviewed.instances[0].model[column])
    }
    XCTAssertEqual(production.instances[0].tint, reviewed.instances[0].tint)
    XCTAssertEqual(production.roughness, reviewed.roughness)
  }

  func testPromotedMeshEnvelopeMasksBridgeAndRestoresTheSameSamples() throws {
    let batch = try LivingWorldPresentation.gardenReedBatch()
    let envelope = LivingWorldPresentation.GardenPlantEnvelope([batch])
    // The envelope is derived from this production mesh, including its wider leaves.
    for vertex in batch.mesh.vertices {
      let p = vertex.position
      XCTAssertGreaterThanOrEqual(p.y, envelope.bottom)
      XCTAssertLessThanOrEqual(p.y, envelope.bottom + envelope.height)
      XCTAssertLessThanOrEqual(length(SIMD2(p.x, p.z)), envelope.radius)
    }
    XCTAssertGreaterThan(envelope.windMargin, 0)
    let roots: [V3] = [V3(20, 0.5, 24), V3(20, 0.5, 28),
      V3(22, 0.5, 24), V3(20, -2, 24), V3(20, 5, 24)]
    func visible(_ construction: PersonalConstruction) -> [Int] {
      let mask = SanctuaryGroundCoverMask(garden: nil, construction: construction)
      return roots.indices.filter { index in
        !mask.excludes(root: roots[index] + V3(0, envelope.bottom, 0),
          height: envelope.height, radius: envelope.radius + envelope.windMargin)
      }
    }
    var construction = PersonalConstruction()
    XCTAssertEqual(visible(construction), [0, 1, 2, 3, 4])
    _ = try construction.apply(.place(.bridge, at: .init(x: 20, y: 1, z: 24),
      yawRadians: .pi / 2, scale: 1.5), expectedRevision: 0)
    XCTAssertEqual(visible(construction), [2, 3, 4])
    let restored = try JSONDecoder().decode(PersonalConstruction.self,
      from: JSONEncoder().encode(construction))
    XCTAssertEqual(visible(restored), [2, 3, 4])
    let placement = try XCTUnwrap(construction.placements.first)
    _ = try construction.apply(.update(placement.id, at: .init(x: 40, y: 1, z: 24),
      yawRadians: 0, scale: 0.5), expectedRevision: construction.revision)
    XCTAssertEqual(visible(construction), [0, 1, 2, 3, 4])
    _ = try construction.apply(.undo, expectedRevision: construction.revision)
    XCTAssertEqual(visible(construction), [2, 3, 4])
    _ = try construction.apply(.undo, expectedRevision: construction.revision)
    XCTAssertEqual(visible(construction), [0, 1, 2, 3, 4])
    print("GardenReedGeometry promoted envelope: bottom=\(envelope.bottom) height=\(envelope.height) radius=\(envelope.radius) windMargin=\(envelope.windMargin)")
  }

  func testCandidateKeepsRootsAndClosedFiniteGeometryWithinBudget() throws {
    let batch = GardenReedStudy.batch()
    let mesh = batch.mesh
    XCTAssertEqual(batch.instances.count, 1)
    XCTAssertEqual(batch.instances[0].tint.w, 8)
    XCTAssertEqual(batch.roughness, 0.92, accuracy: 0.000001)
    XCTAssertEqual(mesh.vertices.count, 416)
    XCTAssertEqual(mesh.indices.count / 3, 784)
    XCTAssertLessThanOrEqual(mesh.vertices.count, 1_000)
    XCTAssertLessThanOrEqual(mesh.indices.count / 3, 800)
    for anchor in GardenReedStudy.rootAnchors {
      XCTAssertTrue(mesh.vertices.contains { V3($0.position.x, $0.position.y, $0.position.z) == anchor })
    }
    struct Edge: Hashable { let low: UInt32; let high: UInt32 }
    var edges: [Edge: (count: Int, orientation: Int)] = [:]
    var signedVolume: Float = 0
    for index in stride(from: 0, to: mesh.indices.count, by: 3) {
      let ids = Array(mesh.indices[index..<(index + 3)])
      let p = ids.map { mesh.vertices[Int($0)].position }
        .map { V3($0.x, $0.y, $0.z) }
      XCTAssertGreaterThan(length_squared(cross(p[1] - p[0], p[2] - p[0])), 1e-20)
      signedVolume += dot(p[0], cross(p[1], p[2])) / 6
      for side in 0..<3 {
        let a = ids[side], b = ids[(side + 1) % 3]
        let key = Edge(low: min(a, b), high: max(a, b))
        let previous = edges[key] ?? (0, 0)
        edges[key] = (previous.count + 1, previous.orientation + (a < b ? 1 : -1))
      }
    }
    XCTAssertTrue(edges.values.allSatisfy { $0.count == 2 && $0.orientation == 0 })
    XCTAssertGreaterThan(signedVolume, 0)
    for vertex in mesh.vertices {
      XCTAssertTrue([vertex.position.x, vertex.position.y, vertex.position.z,
        vertex.normal.x, vertex.normal.y, vertex.normal.z].allSatisfy(\.isFinite))
    }
    let repeatMesh = GardenReedStudy.mesh()
    XCTAssertEqual(mesh.indices, repeatMesh.indices)
    XCTAssertEqual(mesh.vertices.map(\.position), repeatMesh.vertices.map(\.position))
    print("GardenReedGeometry candidate receipt: vertices=\(mesh.vertices.count) triangles=\(mesh.indices.count / 3) sourceBytes=\(mesh.vertices.count * MemoryLayout<Vertex>.stride + mesh.indices.count * 4) signedVolume=\(signedVolume)")
  }

  func testSemanticCornersKeepFiniteConservativeStudyBounds() {
    for height: Float in [0.75, 1.3] {
      for spread: Float in [0.12, 0.32] {
        for bend: Float in [0.04, 0.18] {
          let mesh = GardenReedStudy.mesh(height: height, leafSpread: spread, leafBend: bend)
          XCTAssertEqual(mesh.vertices.count, 416)
          for vertex in mesh.vertices {
            let p = vertex.position
            XCTAssertTrue(p.x.isFinite && p.y.isFinite && p.z.isFinite)
            XCTAssertLessThanOrEqual(abs(p.x), 0.8)
            XCTAssertLessThanOrEqual(abs(p.z), 0.8)
            XCTAssertGreaterThanOrEqual(p.y, -0.12)
            XCTAssertLessThanOrEqual(p.y, 1.6)
          }
        }
      }
    }
  }

  func testDefaultLeafCenterlinesAvoidTheObservedSharpElbow() {
    let mesh = GardenReedStudy.mesh()
    // Four unchanged 44-vertex stems, each followed by two 30-vertex leaves.
    for stem in 0..<4 {
      for leaf in 0..<2 {
        let start = stem * 104 + 44 + leaf * 30
        let centers: [V3] = (0...6).map { ring in
          (0..<4).reduce(V3.zero) { sum, side in
            let p = mesh.vertices[start + ring * 4 + side].position
            return sum + V3(p.x, p.y, p.z) * 0.25
          }
        }
        for span in 0..<5 {
          let a = normalize(centers[span + 1] - centers[span])
          let b = normalize(centers[span + 2] - centers[span + 1])
          XCTAssertGreaterThan(dot(a, b), cos(Float.pi / 12)) // under 15 degrees per join
        }
        let attachment = GardenReedStudy.rootAnchors[stem]
        XCTAssertGreaterThan(centers[0].y, attachment.y)
        // The center cap at each leaf base lies inside its parent's stem centerline.
        let cap = mesh.vertices[start + 28].position
        XCTAssertEqual(cap.x, centers[0].x, accuracy: 0.000001)
        XCTAssertEqual(cap.y, centers[0].y, accuracy: 0.000001)
        XCTAssertEqual(cap.z, centers[0].z, accuracy: 0.000001)
      }
    }
  }

}
