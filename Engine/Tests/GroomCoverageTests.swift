import XCTest
import FieldCore
import FieldCompiler
import simd

final class GroomCoverageTests: XCTestCase {
  func testFilteredStrandsRetainMeanDensityWhenMinified() {
    let near=(0..<2048).map { i in GroomCoverage.sample(SIMD4(Float(i)/2048,0.04,0.8,0.25),footprint:SIMD2(0.003,0.001)) }
    let far=(0..<32).map { i in GroomCoverage.sample(SIMD4(3000+Float(i)/32,0.04,0.8,0.25),footprint:SIMD2(8,0.001)) }
    XCTAssertEqual(near.reduce(0,+)/Float(near.count),0.8,accuracy:0.002)
    XCTAssertTrue(near.contains{$0<0.01});XCTAssertTrue(near.contains{$0>0.99})
    XCTAssertTrue(far.allSatisfy{abs($0-0.8)<0.00001})
    for i in 0..<100 {
      let value=GroomCoverage.sample(SIMD4(Float(i)*31.12,Float(i)/99,0.72,0.3),footprint:SIMD2(0.002,0.004))
      XCTAssertTrue(value.isFinite && (0...1).contains(value))
    }
    XCTAssertEqual(GroomCoverage.sample(SIMD4(0.5,1,0.8,0.3),footprint:SIMD2(0.1,0.01)),0)
    XCTAssertEqual(GroomCoverage.sample(.zero,footprint:SIMD2(repeating:1)),1)
  }
  func testExplicitCoverageSurvivesCompilerReorderingAndRefinement() throws {
    XCTAssertEqual(MemoryLayout<Vertex>.stride,64)
    var mesh=ParametricMesh.surface(u:2,v:2,position:{u,v in V3(u,v,0)})
    for i in mesh.vertices.indices {
      let p=mesh.vertices[i].position
      mesh.vertices[i].groom=SIMD4(3+p.x*2,p.y,0.8,0.2)
      mesh.vertices[i].color.w=0.63
    }
    mesh=MeshProcessing.optimize(mesh)
    let refined=try SculptCompiler.refine(mesh,weights:[],levels:1).0
    for v in refined.vertices {
      XCTAssertEqual(v.groom.x,3+v.position.x*2,accuracy:0.00001)
      XCTAssertEqual(v.groom.y,v.position.y,accuracy:0.00001)
      XCTAssertEqual(v.groom.z,0.8);XCTAssertEqual(v.color.w,0.63)
    }
    let meshlets=MeshletData(mesh)
    XCTAssertFalse(meshlets.descriptors.isEmpty)
    XCTAssertTrue(meshlets.spheres.allSatisfy{$0.w.isFinite && $0.w>0 && $0.w<2})
  }
}
