import FieldCompiler
import FieldCore
import simd
import XCTest
@testable import FieldEngine

final class PreparedSceneUploadTests: XCTestCase {
  private func mesh(offset: V3 = .zero, groom: Float = 0, error: Float = 0) -> Mesh {
    var result = Mesh()
    var a = Vertex(offset + V3(-2, 1, -1), V3(0, 1, 0), V3(repeating: 1))
    var b = Vertex(offset + V3(3, 2, -1), V3(0, 1, 0), V3(repeating: 1))
    var c = Vertex(offset + V3(0, 4, 2), V3(0, 1, 0), V3(repeating: 1))
    a.groom.z = groom; b.groom.z = groom; c.groom.z = groom
    result.triangle(a, b, c)
    result.geometricError = error
    return result
  }

  func testPreparationMatchesProductionBoundsMeshletsAndPackedDescriptorLayout() throws {
    let base = mesh(groom: 0.4)
    let lod = mesh(offset: V3(1, -1, 2), error: 0.03)
    var batch = SceneBatch(name: "prepared", mesh: base, instances: [Instance()], lodMeshes: [lod])
    let prepared = try XCTUnwrap(batch.prepareUpload())

    XCTAssertTrue(prepared.matches(batch))
    XCTAssertEqual(prepared.base.center, V3(0.5, 2.5, 0.5))
    XCTAssertEqual(prepared.base.radius, length(V3(5, 3, 3)) / 2 + 1, accuracy: 0.000001)
    XCTAssertTrue(prepared.base.hasGroomCoverage)
    XCTAssertFalse(prepared.lods[0].hasGroomCoverage)
    XCTAssertEqual(MemoryLayout<PreparedSceneUpload.MeshletDescriptor>.size, 32)
    XCTAssertEqual(MemoryLayout<PreparedSceneUpload.MeshletDescriptor>.stride, 32)

    let expected = MeshletData(base)
    XCTAssertEqual(prepared.base.descriptors.count, expected.descriptors.count)
    XCTAssertEqual(prepared.base.meshletVertices, expected.vertices)
    XCTAssertEqual(prepared.base.meshletTriangles, expected.triangles)
    for (actual, index) in zip(prepared.base.descriptors, expected.descriptors.indices) {
      XCTAssertEqual(actual.ranges, expected.descriptors[index])
      XCTAssertEqual(actual.sphere, expected.spheres[index])
    }
    let expectedLOD = MeshletData(lod)
    XCTAssertEqual(prepared.lods[0].center, V3(1.5, 1.5, 2.5))
    XCTAssertEqual(prepared.lods[0].radius, length(V3(5, 3, 3)) / 2 + 1, accuracy: 0.000001)
    XCTAssertEqual(prepared.lods[0].meshletVertices, expectedLOD.vertices)
    XCTAssertEqual(prepared.lods[0].meshletTriangles, expectedLOD.triangles)
    for (actual, index) in zip(prepared.lods[0].descriptors, expectedLOD.descriptors.indices) {
      XCTAssertEqual(actual.ranges, expectedLOD.descriptors[index])
      XCTAssertEqual(actual.sphere, expectedLOD.spheres[index])
    }
    let expectedBytes = ([prepared.base] + prepared.lods).reduce(0) {
      $0 + $1.descriptors.count * 32 + $1.meshletVertices.count * 4
        + $1.meshletTriangles.count
    }
    XCTAssertEqual(prepared.byteCount, expectedBytes)
  }

  func testInstanceNameAndMaterialChangesRetainPreparation() throws {
    var batch = SceneBatch(name: "source", mesh: mesh(), instances: [Instance()])
    let prepared = try XCTUnwrap(batch.prepareUpload())
    var changed = batch
    changed.name = "renamed"
    changed.instances.append(Instance(position: V3(4, 0, 0)))
    changed.roughness = 0.31
    changed.metallic = 0.6
    changed.doubleSided = true
    XCTAssertNotNil(changed.preparedUpload)
    XCTAssertTrue(prepared.matches(changed))
  }

  func testEveryBaseAndNestedLODMeshMutationInvalidatesPreparation() throws {
    var original = SceneBatch(name: "source", mesh: mesh(), instances: [Instance()],
      lodMeshes: [mesh(error: 0.02)])
    let prepared = try XCTUnwrap(original.prepareUpload())
    var mutations: [(inout SceneBatch) -> Void] = [
      { $0.mesh.vertices[0].position.x += 0.01 },
      { $0.mesh.indices.swapAt(0, 1) },
      { $0.mesh.geometricError += 0.01 },
      { $0.lodMeshes[0].vertices[1].normal.y -= 0.02 },
      { $0.lodMeshes[0].indices.swapAt(1, 2) },
      { $0.lodMeshes[0].geometricError += 0.01 },
      { $0.lodMeshes.append(self.mesh(error: 0.05)) },
      { $0.lodMeshes.removeAll() },
    ]
    for mutate in mutations {
      var candidate = original
      mutate(&candidate)
      XCTAssertNil(candidate.preparedUpload)
      XCTAssertFalse(prepared.matches(candidate))
    }
  }

  func testProceduralGrassUsesItsDedicatedPreparationPath() {
    let blade = GrassBlade(
      anchor: .zero, angle: 0, height: 0.3, width: 0.03, color: V3(repeating: 0.5))
    var batch = SceneBatch(name: "grass", mesh: Mesh(), instances: [Instance()], grass: [blade])
    XCTAssertNil(batch.prepareUpload())
  }
}
