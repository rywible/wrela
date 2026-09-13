import XCTest
import FieldCore
@testable import FieldCompiler

final class SparseReferenceTests:XCTestCase {
  func testSparseBlocksPreserveDenseReferenceIncludingPartialBlocksAndImplicitCuts() throws {
    let loft=Shape.loft(SectionLoft([
      LoftSection(id:"a",z:-0.8,center:.zero,radius:SIMD2(0.2,0.3)),
      LoftSection(id:"b",z:0,center:SIMD2(0.1,0),radius:SIMD2(0.5,0.35)),
      LoftSection(id:"c",z:0.9,center:.zero,radius:SIMD2(0.25,0.15))]))
    let fields=[Shape.sphere(0.5).moved(V3(-0.5,0,0)).blended(.sphere(0.6).moved(V3(0.5,0,0)),radius:0.1),
      loft.cut(.sphere(0.12).moved(V3(0.4,0,0)),radius:0.015)]
    for field in fields {for resolution in [12,24] {
      let bounds=field.bounds.expanded(0.25)
      let sparse=try Mesher.referenceExtraction(field,resolution:resolution,color:V3(repeating:0.5),bounds:bounds,refine:true)
      let dense=try Mesher.referenceExtraction(field,resolution:resolution,color:V3(repeating:0.5),bounds:bounds,refine:true,sparse:false)
      XCTAssertEqual(sparse.mesh.indices,dense.mesh.indices)
      XCTAssertEqual(sparse.mesh.vertices.map(\.position),dense.mesh.vertices.map(\.position))
      XCTAssertEqual(sparse.mesh.vertices.map(\.normal),dense.mesh.vertices.map(\.normal))
      XCTAssertEqual(sparse.refinedVertices,dense.refinedVertices)
      XCTAssertEqual(sparse.heldVertices,dense.heldVertices)
    }}
  }
}
