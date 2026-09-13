import XCTest
import FieldCore
import FieldCompiler
import simd

final class LocalRefinementTests:XCTestCase {
  func testLocalRemeshingKeepsClosedTopologyAndFixedExterior() throws {
    let field=Shape.sphere(1),mesh=try Mesher.compile(field,resolution:32),skin=mesh.vertices.map{_ in SkinWeight(0)}
    let detail=SculptDetail(id:"local-remesh",part:"skin",center:V3(0,0,1),radius:V3(repeating:0.7),edgeLength:0.5,maximumNewVertices:1000,
      projection:SculptProjection(maximumDistance:0.05,tolerance:0.000001,retriangulate:true))
    let (output,weights)=try SculptCompiler.refineLocal(mesh,weights:skin,detail:detail,field:field)
    XCTAssertLessThan(output.indices.count,mesh.indices.count)
    XCTAssertTrue(SurfaceCompiler.isClosed(output))
    XCTAssertEqual(weights.count,output.vertices.count);XCTAssertTrue(weights.allSatisfy{$0.joints.x==0 && $0.weights.x==1})
    func exterior(_ m:Mesh)->Set<V3> {Set(m.vertices.compactMap{v in let p=V3(v.position.x,v.position.y,v.position.z);return length(p-detail.center)>detail.radius.x ? p:nil})}
    XCTAssertEqual(exterior(output),exterior(mesh))
  }
  func testOptionalRetriangulationRepairsSliversWithoutMovingVerticesOrSkin() throws {
    var mesh=Mesh();mesh.vertices=[V3(0,0,0),V3(0.1,0,0),V3(0.05,0.0001,0),V3(0.05,-0.1,0)].map{Vertex($0,V3(0,0,1),V3(repeating:0.5))}
    mesh.indices=[0,1,2,1,0,3]
    let skin=mesh.vertices.map{_ in SkinWeight(0)},field=Shape.box(V3(1,1,1)).moved(V3(0,0,-1))
    let detail=SculptDetail(id:"sliver",part:"skin",center:.zero,radius:V3(repeating:1),edgeLength:0.5,maximumNewVertices:100,
      projection:SculptProjection(maximumDistance:0.01,tolerance:0.000001,retriangulate:true))
    let (out,weights)=try SculptCompiler.refineLocal(mesh,weights:skin,detail:detail,field:field)
    XCTAssertEqual(out.vertices.map(\.position),mesh.vertices.map(\.position))
    XCTAssertEqual(weights.map(\.joints),skin.map(\.joints));XCTAssertEqual(weights.map(\.weights),skin.map(\.weights))
    XCTAssertNotEqual(out.indices,mesh.indices)
    for i in stride(from:0,to:out.indices.count,by:3) {
      let p=(0..<3).map{out.vertices[Int(out.indices[i+$0])].position}
      XCTAssertGreaterThan(length(cross(V3(p[1].x-p[0].x,p[1].y-p[0].y,0),V3(p[2].x-p[0].x,p[2].y-p[0].y,0))),0.004)
    }
    XCTAssertEqual(try SculptCompiler.refineLocal(mesh,weights:skin,detail:detail,field:field).0.indices,out.indices)
  }
  func testRefinementCoordinatesDuplicatedSeamEdgesWithoutBlendingAttributes() throws {
    var mesh=Mesh()
    let red=V3(1,0,0),blue=V3(0,0,1),normal=V3(0,0,1)
    mesh.triangle(Vertex(V3(0,0,0),normal,red),Vertex(V3(1,0,0),normal,red),Vertex(V3(0,1,0),normal,red))
    mesh.triangle(Vertex(V3(1,0,0),normal,blue),Vertex(V3(1,1,0),normal,blue),Vertex(V3(0,1,0),normal,blue))
    let detail=SculptDetail(id:"seam",part:"skin",center:V3(0.12,0.12,0),radius:V3(repeating:0.03),edgeLength:0.08,maximumNewVertices:3000)
    let (output,_)=try SculptCompiler.refineLocal(mesh,weights:[],detail:detail)
    var redSeam:Set<V3>=[],blueSeam:Set<V3>=[]
    for v in output.vertices {
      XCTAssertTrue(v.color.x==1 || v.color.z==1,"Attribute seam was blended")
      let p=V3(v.position.x,v.position.y,v.position.z)
      if abs(p.x+p.y-1)<0.000001 {
        if v.color.x==1 {redSeam.insert(p)} else {blueSeam.insert(p)}
      }
    }
    XCTAssertGreaterThan(redSeam.count,2)
    XCTAssertEqual(redSeam,blueSeam,"Only one side of a duplicated geometric edge was refined")
  }
  func testLocalFieldProjectionRecoversCurvatureAndRejectsWrongCorrespondence() throws {
    let field=Shape.sphere(1),input=try Mesher.compile(field,resolution:12)
    let skin=input.vertices.map{_ in SkinWeight(0)}
    let detail=SculptDetail(id:"curvature",part:"skin",center:V3(0,0,1),radius:V3(repeating:0.5),edgeLength:0.06,
      maximumNewVertices:4000,projection:SculptProjection(maximumDistance:0.06,tolerance:0.00001))
    let (mesh,weights)=try SculptCompiler.refineLocal(input,weights:skin,detail:detail,field:field)
    XCTAssertEqual(mesh.vertices.count,weights.count)
    var interior=0,exterior=0
    for i in mesh.vertices.indices {
      let v=mesh.vertices[i],p=V3(v.position.x,v.position.y,v.position.z)
      if length(p-detail.center)<0.3 {
        XCTAssertLessThanOrEqual(abs(field.value(at:p)),0.000011)
        XCTAssertGreaterThan(dot(V3(v.normal.x,v.normal.y,v.normal.z),normalize(p)),0.9999)
        interior+=1
      }
      if i<input.vertices.count && length(p-detail.center)>0.51 {XCTAssertEqual(v.position,input.vertices[i].position);exterior+=1}
    }
    XCTAssertGreaterThan(interior,20);XCTAssertGreaterThan(exterior,20)
    XCTAssertThrowsError(try SculptCompiler.refineLocal(input,weights:skin,detail:detail))
    XCTAssertThrowsError(try SculptCompiler.refineLocal(input,weights:skin,detail:detail,field:.sphere(0.5)))
    XCTAssertEqual(input.vertices.count,skin.count)
  }
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
