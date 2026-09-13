import XCTest
import simd
@testable import FieldCore
@testable import FieldCompiler

final class ProjectedRetriangulationTests:XCTestCase {
  func fixture()->(Mesh,Mesh) {
    let positions=[V3(0.19911963,0.19529305,0.9661238),V3(0.2457178,0.20443703,0.96771485),V3(0.25291592,0.19628148,0.9258264),V3(0.19561955,0.24286821,0.928195)]
    var input=Mesh();input.vertices=positions.map{Vertex($0,normalize($0),V3(repeating:0.5))};input.indices=[0,1,2,2,3,0]
    var projected=input;for i in projected.vertices.indices {projected.vertices[i].position=SIMD4(normalize(positions[i]),1)}
    return (input,projected)
  }
  func testProjectionAwareDiagonalPreservesVerticesAndBoundary() {
    let (input,projected)=fixture(),detail=SculptDetail(id:"patch",part:"body",center:V3(0.23,0.22,0.95),radius:V3(repeating:0.2),edgeLength:0.1,maximumNewVertices:100)
    let result=SculptCompiler.repairProjectedTriangles(input,projected:projected,detail:detail,field:.sphere(1))
    XCTAssertEqual(result.flips,1)
    XCTAssertEqual(result.reference.vertices.map(\.position),input.vertices.map(\.position))
    XCTAssertEqual(result.projected.vertices.map(\.position),projected.vertices.map(\.position))
    XCTAssertEqual(result.reference.indices,result.projected.indices)
    XCTAssertNotEqual(result.reference.indices,input.indices)
    func edges(_ mesh:Mesh)->Set<UInt64> {
      var counts:[UInt64:Int]=[:]
      for t in stride(from:0,to:mesh.indices.count,by:3) {for j in 0..<3 {
        let a=mesh.indices[t+j],b=mesh.indices[t+(j+1)%3],key=UInt64(min(a,b))<<32|UInt64(max(a,b));counts[key,default:0]+=1
      }}
      return Set(counts.filter{$0.value==1}.keys)
    }
    XCTAssertEqual(edges(input),edges(result.reference))
    for t in stride(from:0,to:result.projected.indices.count,by:3) {
      func area(_ m:Mesh)->V3 {let p=(0..<3).map{j->V3 in let v=m.vertices[Int(m.indices[t+j])].position;return V3(v.x,v.y,v.z)};return cross(p[1]-p[0],p[2]-p[0])}
      XCTAssertGreaterThan(dot(area(result.reference),area(result.projected)),0)
    }
    var locked=detail;locked.center=V3(10,10,10)
    XCTAssertEqual(SculptCompiler.repairProjectedTriangles(input,projected:projected,detail:locked,field:.sphere(1)).flips,0)
  }
}
