import XCTest
import FieldCore
import FieldCompiler
import simd

final class LocalRefinementTests:XCTestCase {
  func testLocalPatchInsideACoarseFacePreservesSurfaceAndSkinWithoutCracks() throws {
    let input=ParametricMesh.surface(u:2,v:2,position:{u,v in V3(u*2-1,0,v*2-1)},tint:{u,v in V3(u,v,0.5)})
    let skin=input.vertices.map{v->SkinWeight in var s=SkinWeight(0);let x=(v.position.x+1)/2;s.joints=SIMD4(0,1,0,0);s.weights=SIMD4(1-x,x,0,0);return s}
    let detail=SculptDetail(id:"small-feature",part:"skin",center:V3(0.23,0,0.39),radius:V3(0.12,0.1,0.12),edgeLength:0.035,maximumNewVertices:3000)
    let (mesh,weights)=try SculptCompiler.refineLocal(input,weights:skin,detail:detail)
    XCTAssertGreaterThan(mesh.vertices.count,input.vertices.count+30)
    XCTAssertLessThan(mesh.vertices.count,1000) // Global 35 mm spacing needs thousands.
    var edges:[UInt64:Int]=[:],area:Float=0
    for i in stride(from:0,to:mesh.indices.count,by:3) {
      let ids=(0..<3).map{mesh.indices[i+$0]},p=ids.map{v->V3 in let p=mesh.vertices[Int(v)].position;return V3(p.x,p.y,p.z)}
      let n=cross(p[1]-p[0],p[2]-p[0]);XCTAssertGreaterThan(length(n),1e-10);area+=length(n)/2
      let local=p.map{($0-detail.center)/detail.radius},b=CostumeCompiler.closestBarycentric(.zero,local[0],local[1],local[2])
      let inside=length_squared(local[0]*b.x+local[1]*b.y+local[2]*b.z)<=1
      for j in 0..<3 {
        let a=ids[j],b=ids[(j+1)%3],key=UInt64(min(a,b))<<32|UInt64(max(a,b));edges[key,default:0]+=1
        if inside {XCTAssertLessThanOrEqual(length(p[j]-p[(j+1)%3]),detail.edgeLength*1.0001)}
      }
    }
    XCTAssertEqual(area,4,accuracy:0.0001)
    for (edge,count) in edges {
      XCTAssertTrue(count==1 || count==2)
      if count==1 {let p=(mesh.vertices[Int(edge>>32)].position+mesh.vertices[Int(edge & 0xffffffff)].position)/2;XCTAssertTrue(abs(p.x)>0.9999 || abs(p.z)>0.9999,"Interior crack")}
    }
    for (v,s) in zip(mesh.vertices,weights) {
      XCTAssertEqual(v.position.y,0)
      var expected:Float=0
      for i in 0..<4 where s.joints[i]==1 {expected += s.weights[i]}
      XCTAssertEqual(expected,(v.position.x+1)/2,accuracy:0.0001)
    }
    var craft=CreatureCraft();craft.detailPatches=[detail]
    XCTAssertEqual(try JSONDecoder().decode(CreatureCraft.self,from:JSONEncoder().encode(craft)),craft)
  }
  func testClosedSurfaceRemainsClosedAndBudgetFailureDoesNotModifyInput() throws {
    var mesh=Mesh()
    let positions=[V3(0,0,0),V3(1,0,0),V3(0,1,0),V3(0,0,1)]
    mesh.vertices=positions.map{Vertex($0,normalize($0-V3(repeating:0.25)),V3(repeating:0.5))}
    mesh.indices=[0,2,1,0,1,3,0,3,2,1,2,3]
    XCTAssertTrue(SurfaceCompiler.isClosed(mesh))
    var detail=SculptDetail(id:"face",part:"skin",center:V3(0.2,0.2,0),radius:V3(repeating:0.12),edgeLength:0.04,maximumNewVertices:2000)
    let (refined,_)=try SculptCompiler.refineLocal(mesh,weights:[],detail:detail)
    XCTAssertTrue(SurfaceCompiler.isClosed(refined))
    detail.maximumNewVertices=1
    XCTAssertThrowsError(try SculptCompiler.refineLocal(mesh,weights:[],detail:detail)) {XCTAssertTrue($0.localizedDescription.contains("budget"))}
    XCTAssertEqual(mesh.vertices.count,4);XCTAssertEqual(mesh.indices.count,12)
  }
}
