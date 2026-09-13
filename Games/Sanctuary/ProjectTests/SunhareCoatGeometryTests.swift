import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import SanctuaryContent
import XCTest
import simd
@testable import SanctuaryProject

/// Exercises the registered source/compiler and actual parent rig. These tests
/// qualify attachment and boundedness, not native fur identity or GPU cost.
final class SunhareCoatGeometryTests: XCTestCase {
  private typealias Part = LivingWorldPresentation.PartDesign
  private static let prefix = "Wildlife sunhare "
  private struct Specimen {
    let generator: AssetGenerator
    let base: [Part]
    let coat: SanctuarySunhareCoatDesign.Recipe
    let off: [SceneBatch]
    let on: [SceneBatch]
  }
  private static let specimen: Result<Specimen, Error> = Result {
    guard let generator = LivingWorldPresentation.generators.first(where: { $0.id == "sunhare" }),
      let compile = generator.compile else { throw RuntimeError.message("Missing Sunhare generator") }
    let base = try SanctuarySunhareDesign.validatedParts()
    let off = try compile(AssetSource(id: "sunhare", name: "Sunhare", generator: "sunhare"))
    let on = try compile(AssetSource(id: "sunhare", name: "Sunhare", generator: "sunhare",
      parameters: ["furStudy": 1]))
    return Specimen(generator: generator, base: base,
      coat: try SanctuarySunhareCoatDesign.compile(base: base), off: off, on: on)
  }
  private func point(_ p: SIMD4<Float>) -> V3 { V3(p.x, p.y, p.z) }
  private func assertSameMesh(_ a: Mesh, _ b: Mesh, file: StaticString = #filePath, line: UInt = #line) {
    XCTAssertEqual(a.indices, b.indices, file: file, line: line)
    XCTAssertEqual(a.vertices.map(\.position), b.vertices.map(\.position), file: file, line: line)
    XCTAssertEqual(a.vertices.map(\.normal), b.vertices.map(\.normal), file: file, line: line)
    XCTAssertEqual(a.vertices.map(\.color), b.vertices.map(\.color), file: file, line: line)
    XCTAssertEqual(a.vertices.map(\.groom), b.vertices.map(\.groom), file: file, line: line)
  }

  func testRegisteredOptInPreservesEveryBaseVertexAndSeparateMaterialStudy() throws {
    let sample = try Self.specimen.get()
    XCTAssertEqual(sample.generator.controls.first { $0.key == "furStudy" }?.initial, 0)
    XCTAssertEqual(sample.off.count, 14)
    XCTAssertEqual(sample.on.count, sample.off.count + 4)
    let explicitOff = try XCTUnwrap(sample.generator.compile)(AssetSource(id: "sunhare",
      name: "Sunhare", generator: "sunhare", parameters: ["furStudy": 0]))
    let onByID = Dictionary(uniqueKeysWithValues: sample.on.map { ($0.name, $0) })
    for (omitted, explicit) in zip(sample.off, explicitOff) {
      XCTAssertEqual(omitted.name, explicit.name)
      assertSameMesh(omitted.mesh, explicit.mesh)
      assertSameMesh(omitted.mesh, try XCTUnwrap(onByID[omitted.name]).mesh)
    }
    let source = AssetSource(id: "sunhare", name: "Sunhare", generator: "sunhare",
      parameters: ["furStudy": 1, "coatStudy": 1])
    let restored = try JSONDecoder().decode(AssetSource.self, from: JSONEncoder().encode(source))
    XCTAssertEqual(restored.parameters, source.parameters)
    let both = try SanctuarySunhareDesign.validatedParts(restored.parameters)
    let furOnly = try SanctuarySunhareDesign.validatedParts(["furStudy": 1])
    for (before, after) in zip(furOnly, both) {
      XCTAssertEqual(before.name, after.name)
      XCTAssertEqual(before.parent, after.parent)
      XCTAssertEqual(before.color, after.color)
      XCTAssertEqual(before.roughness, after.roughness)
      XCTAssertEqual(before.shape.bounds.min, after.shape.bounds.min)
      XCTAssertEqual(before.shape.bounds.max, after.shape.bounds.max)
      if let first = before.proceduralMesh, let second = after.proceduralMesh { assertSameMesh(first, second) }
    }
    XCTAssertEqual(furOnly.first { $0.name == Self.prefix + "body" }?.material, 9)
    XCTAssertEqual(both.first { $0.name == Self.prefix + "body" }?.material, 15)
    XCTAssertTrue(both.suffix(4).allSatisfy { $0.material == 13 && !$0.castsShadow })
  }

  /// A ray from a buried root along its source normal must meet the actual
  /// compiled parent close by; testing only the source field could miss a mesh gap.
  private func exitDistance(origin: V3, direction: V3, mesh: Mesh) -> Float? {
    var nearest: Float?
    for i in stride(from: 0, to: mesh.indices.count, by: 3) {
      let a = point(mesh.vertices[Int(mesh.indices[i])].position)
      let b = point(mesh.vertices[Int(mesh.indices[i + 1])].position)
      let c = point(mesh.vertices[Int(mesh.indices[i + 2])].position)
      let e1 = b - a, e2 = c - a, p = cross(direction, e2)
      let determinant = dot(e1, p)
      if abs(determinant) < 1e-10 { continue }
      let inverse = 1 / determinant, offset = origin - a
      let u = dot(offset, p) * inverse
      if u < -1e-5 || u > 1.00001 { continue }
      let q = cross(offset, e1), v = dot(direction, q) * inverse
      if v < -1e-5 || u + v > 1.00001 { continue }
      let t = dot(e2, q) * inverse
      if t >= 0 && (nearest == nil || t < nearest!) { nearest = t }
    }
    return nearest
  }

  func testActualCompiledRootsMeetParentsAndRespectFaceLiningAndSoleExclusions() throws {
    let sample = try Self.specimen.get()
    let parentMeshes = Dictionary(uniqueKeysWithValues: sample.off.map { ($0.name, $0.mesh) })
    let parents = Dictionary(uniqueKeysWithValues: sample.base.map { ($0.name, $0) })
    let exclusions = sample.base.filter { ["eyes", "muzzle", "nose"].contains(String($0.name.dropFirst(Self.prefix.count))) }
    XCTAssertEqual(sample.coat.guides.count, 352)
    XCTAssertGreaterThan(sample.coat.exposedVertexFraction, 0.55)
    XCTAssertLessThanOrEqual(sample.coat.maximumReductionError, 0.001)
    print("Sunhare coat compiled exposure fraction \(sample.coat.exposedVertexFraction), maximum fine-to-retained sample deviation \(sample.coat.maximumReductionError) m, raw bytes \(sample.coat.meshBytes)")
    for coat in sample.coat.parts {
      let parentID = try XCTUnwrap(coat.parent), parent = try XCTUnwrap(parents[parentID])
      let mesh = try XCTUnwrap(coat.proceduralMesh), parentMesh = try XCTUnwrap(parentMeshes[parentID])
      let guides = sample.coat.guides.filter { $0.parent == parentID }
      for strand in 0..<guides.count {
        let guide = guides[strand], start = guide.vertexStart
        XCTAssertLessThanOrEqual(start + guide.vertexCount, mesh.vertices.count)
        let root = mesh.vertices[start..<(start + guide.rootWidth)]
        for vertex in root { XCTAssertLessThanOrEqual(parent.shape.value(at: point(vertex.position)), 0) }
        let center = root.map { point($0.position) }.reduce(V3.zero, +) / Float(root.count)
        let exit = try XCTUnwrap(exitDistance(origin: center, direction: guides[strand].normal, mesh: parentMesh), parentID)
        XCTAssertLessThan(exit, 0.012, "Wider root group must exit the rendered parent within12 mm")
      }
      for vertex in mesh.vertices {
        let p = point(vertex.position)
        XCTAssertGreaterThan(p.y, 0.31, "No sole or low-limb fibres")
        for excluded in exclusions { XCTAssertGreaterThan(excluded.shape.value(at: p), 0.008, excluded.name) }
        XCTAssertGreaterThan(vertex.groom.z, 0, "Undercoat uses the existing filtered coverage path")
        XCTAssertGreaterThanOrEqual(vertex.groom.y, 0)
        XCTAssertLessThanOrEqual(vertex.groom.y, 1)
        if parentID.contains("ear-") {
          XCTAssertGreaterThan(p.z, parent.pivot.z + 0.006, "Lining is on the front, -Z side")
        }
      }
      // Real production coverage, sampled at coarse footprints, must retain
      // interior fibre coverage while fading the tips. This is not a rendered
      // softness or anti-aliasing approval.
      for strand in 0..<guides.count {
        let guide = guides[strand], start = guide.vertexStart
        let interior = mesh.vertices[start + guide.interiorVertex].groom
        let tip = mesh.vertices[start + guide.tipVertex].groom
        let footprint = SIMD2<Float>(2, 0.05)
        XCTAssertGreaterThan(GroomCoverage.sample(interior, footprint: footprint), 0.7)
        XCTAssertLessThan(GroomCoverage.sample(tip, footprint: footprint), 0.01)
        // Probe actual retained side vertices, not a duplicate envelope formula.
        // Edges stay on the groom path but expose the substrate even when the
        // individual fibre intervals are unresolved. Center duty stays intact.
        for offset in 0..<guide.vertexCount where offset % guide.rootWidth == 0 {
          let left = mesh.vertices[start + offset].groom
          let right = mesh.vertices[start + offset + guide.rootWidth - 1].groom
          for edge in [left, right] {
            XCTAssertGreaterThan(edge.z, 0, "Zero is the non-groom sentinel")
            for span: Float in [0.05, 1, 3.052, 8] {
              XCTAssertLessThan(GroomCoverage.sample(edge, footprint: SIMD2(span, 0.05)), 0.003,
                "Side duty must not restore an opaque polygon border")
            }
          }
        }
      }
      for guide in guides {
        if let triangle = guide.outerTriangle {
          let indices = parentMesh.indices[(triangle * 3)..<(triangle * 3 + 3)]
          let centroid = indices.map { point(parentMesh.vertices[Int($0)].position) }.reduce(V3.zero, +) / 3
          XCTAssertLessThan(length(centroid - guide.surface), 1e-6, "Root chosen on actual outer-ear triangle")
          XCTAssertGreaterThan(guide.normal.z, 0.25)
        }
      }
    }
  }

  func testBoundedDeterministicCompilerAcrossSemanticCornersAndCacheEviction() throws {
    let corners: [[String: Float]] = [[:], ["headWidth": 0.88, "cheeks": 1.15],
      ["headWidth": 1.15, "cheeks": 0.85], ["earLength": 0.28, "earSpread": 25],
      ["earLength": 0.50, "earSpread": 8], ["bodyRoundness": 0.85], ["bodyRoundness": 1.15]]
    for var parameters in corners {
      parameters["furStudy"] = 1
      let compiled: [Part]
      do { compiled = try SanctuarySunhareDesign.validatedParts(parameters) }
      catch {
        XCTFail("Coat semantic corner \(parameters.sorted { $0.key < $1.key }) failed: \(error)")
        throw error
      }
      let coat = compiled.suffix(4)
      var triangles = 0, vertices = 0, bytes = 0
      for part in coat {
        let mesh = try XCTUnwrap(part.proceduralMesh)
        vertices += mesh.vertices.count; triangles += mesh.indices.count / 3
        bytes += mesh.vertices.count * MemoryLayout<Vertex>.stride + mesh.indices.count * 4
        for vertex in mesh.vertices {
          XCTAssertTrue((0..<4).allSatisfy { vertex.position[$0].isFinite && vertex.normal[$0].isFinite })
          XCTAssertEqual(length(point(vertex.normal)), 1, accuracy: 0.00002)
        }
      }
      XCTAssertGreaterThanOrEqual(vertices, 352 * 15)
      XCTAssertGreaterThanOrEqual(triangles, 352 * 16)
      XCTAssertLessThanOrEqual(triangles, SanctuarySunhareCoatDesign.maximumTriangles)
      XCTAssertLessThanOrEqual(bytes, SanctuarySunhareCoatDesign.maximumMeshBytes)
      XCTAssertLessThanOrEqual(SanctuarySunhareDesign.cacheStatistics.recipes, 4)
      XCTAssertLessThanOrEqual(SanctuarySunhareDesign.cacheStatistics.coatBytes, SanctuarySunhareCoatDesign.maximumCacheBytes)
    }
    let regenerated = try SanctuarySunhareDesign.validatedParts(["furStudy": 1]).suffix(4)
    for (first, second) in zip(try Self.specimen.get().coat.parts, regenerated) {
      assertSameMesh(try XCTUnwrap(first.proceduralMesh), try XCTUnwrap(second.proceduralMesh))
    }
    for invalid: [String: Float] in [["furStudy": 0.5], ["furStudy": .nan], ["earLength": 100], ["unknownCoatKey": 1]] {
      XCTAssertThrowsError(try SanctuarySunhareDesign.validatedParts(invalid))
    }
    XCTAssertThrowsError(try SanctuarySunhareCoatDesign.compile(base: []))
  }

  func testActualGroomVerticesInheritAnimatedParentsWithoutChangingBaseRig() throws {
    let sample = try Self.specimen.get(), animation = try XCTUnwrap(sample.generator.animation)
    let onJoints = sample.generator.parts(["furStudy": 1]).compactMap(\.joint)
    let offJoints = sample.generator.parts([:]).compactMap(\.joint)
    try PartRig.validate(onJoints)
    XCTAssertEqual(onJoints.count, offJoints.count + 4)
    let state = animation.initialize(17)
    let motion = MotionParameters(Dictionary(uniqueKeysWithValues: animation.controls.map { ($0.key, $0.initial) }))
    var animatedSamples = 0
    for clip in ["idle", "hop"] {
      for time: Float in [0, 0.187, 0.3995, 0.612, 0.731, 0.85] {
        let poses = animation.poses(clip, time, state, motion)
        let on = PartRig.matrices(onJoints, poses: poses), off = PartRig.matrices(offJoints, poses: poses)
        for joint in offJoints {
          let before = try XCTUnwrap(off[joint.id]), after = try XCTUnwrap(on[joint.id])
          for column in 0..<4 { XCTAssertEqual(before[column], after[column]) }
        }
        for coat in sample.coat.parts {
          let own = try XCTUnwrap(on[coat.name]), parent = try XCTUnwrap(on[try XCTUnwrap(coat.parent)])
          for vertex in try XCTUnwrap(coat.proceduralMesh).vertices {
            XCTAssertLessThan(length(own * vertex.position - parent * vertex.position), 1e-6)
          }
          if length(parent.columns.3) > 0.01 || parent.columns.0 != matrix_identity_float4x4.columns.0 { animatedSamples += 1 }
        }
      }
    }
    XCTAssertGreaterThan(animatedSamples, 0, "Must exercise actual moving parent transforms")
  }
}
