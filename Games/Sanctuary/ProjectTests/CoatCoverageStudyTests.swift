import FieldCompiler
import FieldCore
import FieldEngine
import XCTest
import simd
@testable import SanctuaryProject

/// Validates real calibration inputs; GPU sample masks still require native readback.
final class CoatCoverageStudyTests: XCTestCase {
  private func compiled(_ parameters: [String: Float] = [:]) throws -> [SceneBatch] {
    let generators = SanctuaryProject.make().generators.filter { $0.id == "coat-coverage" }
    XCTAssertEqual(generators.count, 1)
    let generator = try XCTUnwrap(generators.first)
    return try XCTUnwrap(generator.compile)(AssetSource(id: generator.id,
      name: generator.name, generator: generator.id, parameters: parameters))
  }
  private func xyz(_ p: SIMD4<Float>) -> V3 { V3(p.x,p.y,p.z) }

  func testRegisteredModesPreserveGeometryAndCoveragePipelineInputs() throws {
    let analytic = try compiled()
    XCTAssertEqual(analytic.count, 3)
    for mode: Float in [0, 2] {
      let variant = try compiled(["coverageMode": mode])
      for (a,b) in zip(analytic,variant) {
        XCTAssertEqual(a.name,b.name)
        XCTAssertEqual(a.mesh.indices,b.mesh.indices)
        XCTAssertEqual(a.mesh.vertices.map(\.position),b.mesh.vertices.map(\.position))
        XCTAssertEqual(a.mesh.vertices.map(\.normal),b.mesh.vertices.map(\.normal))
        XCTAssertEqual(a.mesh.vertices.map(\.color),b.mesh.vertices.map(\.color))
        XCTAssertEqual(a.mesh.vertices.map(\.groom),b.mesh.vertices.map(\.groom))
      }
      XCTAssertTrue(variant.dropFirst().allSatisfy { $0.instances[0].tint.w == (mode == 0 ? 13 : 31) })
    }
    XCTAssertTrue(analytic.dropFirst().allSatisfy { $0.instances[0].tint.w == 30 })
    for batch in analytic.dropFirst() {
      // Renderer selects hasGroomCoverage from these same mesh values in all modes.
      XCTAssertTrue(batch.mesh.vertices.allSatisfy { $0.groom.z > 0 })
    }
    XCTAssertTrue(analytic[0].mesh.vertices.allSatisfy { $0.groom == .zero })
    XCTAssertEqual(Set(analytic[0].mesh.vertices.map { $0.color.x }),Set([Float(0),0.18,1]))
  }

  func testActualPatchDimensionsNormalsAndResolvedMeanCoverage() throws {
    for density: Float in [0.25,0.5,0.75,0.94] {
      let batches = try compiled(["density": density])
      for (index,batch) in batches.dropFirst().enumerated() {
        let mesh = batch.mesh
        XCTAssertEqual(mesh.vertices.count,225)
        XCTAssertEqual(mesh.indices.count,1152)
        for (i,v) in mesh.vertices.enumerated() {
          let t = Float(i / 9) / 24
          XCTAssertEqual(v.position.y,0.025 + t * 0.067,accuracy:1e-7)
          XCTAssertEqual(v.position.z,index == 0 ? 0 : SanctuaryCoatCoverageStudy.elevation(t),accuracy:1e-7)
          XCTAssertEqual(length(xyz(v.normal)),1,accuracy:1e-6)
          XCTAssertGreaterThan(v.normal.z,0)
          XCTAssertEqual(v.groom.y,t)
          XCTAssertEqual(v.groom.z,density)
        }
        for i in stride(from:0,to:mesh.indices.count,by:3) {
          let a=xyz(mesh.vertices[Int(mesh.indices[i])].position)
          let b=xyz(mesh.vertices[Int(mesh.indices[i+1])].position)
          let c=xyz(mesh.vertices[Int(mesh.indices[i+2])].position)
          XCTAssertGreaterThan(cross(b-a,c-a).z,0)
        }
        let interior=mesh.vertices[6*9+4].groom
        XCTAssertEqual(GroomCoverage.sample(interior,footprint:SIMD2(8,0.001)),density,accuracy:0.00001)
        XCTAssertEqual(GroomCoverage.sample(mesh.vertices[24*9+4].groom,footprint:SIMD2(8,0.001)),0)
      }
      let bytes=batches.reduce(0) { $0 + $1.mesh.vertices.count * MemoryLayout<Vertex>.stride + $1.mesh.indices.count * 4 }
      XCTAssertLessThan(bytes,40_000)
    }
  }

  func testInvalidCalibrationInputsRejectAndReferenceIsDeterministic() throws {
    for parameters: [String: Float] in [["coverageMode":0.5],["coverageMode":3],
      ["density":0],["density":Float.nan],["period":0],["period":Float.infinity]] {
      XCTAssertThrowsError(try compiled(parameters))
    }
    let first=try compiled(), second=try compiled()
    for (a,b) in zip(first,second) {
      XCTAssertEqual(a.mesh.vertices.map(\.position),b.mesh.vertices.map(\.position))
      XCTAssertEqual(a.mesh.vertices.map(\.groom),b.mesh.vertices.map(\.groom))
      XCTAssertEqual(a.mesh.indices,b.mesh.indices)
    }
  }
}
