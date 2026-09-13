import XCTest
import FieldCore
import FieldCompiler
import simd

final class SurfaceOrientationTests:XCTestCase {
  func testClosedEdgeIncidenceDoesNotCertifyOutwardGeometry() throws {
    var mesh=Mesh()
    let center=V3(repeating:0.25),field=Shape.sphere(0.5).moved(center)
    mesh.vertices=[V3.zero,V3(1,0,0),V3(0,1,0),V3(0,0,1)].map{Vertex($0,normalize($0-center),V3(repeating:1))}
    mesh.indices=[0,2,1,0,1,3,0,3,2,1,2,3]
    XCTAssertTrue(SurfaceCompiler.isClosed(mesh))
    XCTAssertEqual(SurfaceCompiler.sampledInvertedTriangles(mesh,field:field),0)
    for i in stride(from:0,to:mesh.indices.count,by:3) {mesh.indices.swapAt(i,i+1)}
    XCTAssertTrue(SurfaceCompiler.isClosed(mesh))
    XCTAssertEqual(SurfaceCompiler.sampledInvertedTriangles(mesh,field:field),4)
  }
  func testReferenceRootsImproveFieldAgreementAndPreserveTriangleOrientation() throws {
    let shapes=[Shape.sphere(0.4),Shape.sphere(0.5).cut(Shape.sphere(0.28).moved(V3(0.3,0.15,-0.25)),radius:0.02)]
    for field in shapes {
      let linear=try Mesher.reference(field,resolution:16),result=try Mesher.referenceRecovery(field,resolution:16),mesh=result.mesh
      XCTAssertEqual(mesh.indices,linear.indices)
      XCTAssertGreaterThan(result.refinedVertices,0)
      func residual(_ mesh:Mesh)->Float {mesh.vertices.reduce(Float(0)){sum,v in sum+abs(field.value(at:V3(v.position.x,v.position.y,v.position.z)))}/Float(mesh.vertices.count)}
      XCTAssertLessThan(residual(mesh),residual(linear)*0.2)
      XCTAssertTrue(SurfaceCompiler.isClosed(MeshProcessing.optimize(mesh)))
      for i in stride(from:0,to:mesh.vertices.count,by:3) {
        func area(_ m:Mesh)->V3 {let p=(0..<3).map{j->V3 in let v=m.vertices[i+j].position;return V3(v.x,v.y,v.z)};return cross(p[1]-p[0],p[2]-p[0])}
        let a=area(linear),b=area(mesh)
        if length_squared(a)>1e-20 {XCTAssertGreaterThan(dot(a,b),0);XCTAssertGreaterThan(length_squared(b),length_squared(a)*0.0025)}
      }
      XCTAssertEqual(try Mesher.referenceRecovery(field,resolution:16).mesh.vertices.map(\.position),mesh.vertices.map(\.position))
    }
  }

}
