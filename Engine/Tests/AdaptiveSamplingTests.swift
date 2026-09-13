import XCTest
import simd
import FieldCore
@testable import FieldCompiler

final class AdaptiveSamplingTests:XCTestCase {
  func welded(_ mesh:Mesh)->Mesh {
    var output=Mesh(),ids:[V3:UInt32]=[:],remap:[UInt32]=[]
    for v in mesh.vertices {
      let p=V3(v.position.x,v.position.y,v.position.z)
      if let id=ids[p] {remap.append(id)} else {let id=UInt32(output.vertices.count);ids[p]=id;remap.append(id);output.vertices.append(v)}
    }
    output.indices=mesh.indices.map{remap[Int($0)]};return output
  }
  func testLocalDensityRemainsClosedAndRecoversSmallComponent() throws {
    let field=Shape.sphere(0.5).joined(.sphere(0.035).moved(V3(0.673,0.071,0.029)))
    let policy=SurfaceSampling(baseEdgeLength:0.16,regions:[SurfaceSamplingRegion(id:"tiny",center:V3(0.67,0.07,0.03),radius:V3(repeating:0.10),edgeLength:0.018)],maximumTetrahedra:100_000)
    let grid=try AdaptiveTetrahedra.build(field,bounds:field.bounds.expanded(0.03),policy:policy)
    XCTAssertGreaterThan(grid.subdivisions,20);XCTAssertLessThan(grid.cells.count,100_000)
    for cell in grid.cells {
      let limit=policy.edgeLength(in:cell.bounds)
      for (a,b) in AdaptiveTetrahedra.edges {XCTAssertLessThanOrEqual(length(grid.points[cell.vertices[a]]-grid.points[cell.vertices[b]]),limit*1.00001)}
    }
    let mesh=try Mesher.sampled(field,policy:policy),closed=welded(mesh)
    XCTAssertTrue(SurfaceCompiler.isClosed(closed))
    XCTAssertGreaterThan(closed.vertices.filter{$0.position.x>0.6}.count,30)
    XCTAssertLessThan(SurfaceCompiler.sampledInvertedTriangles(closed,field:field),1)
    let again=try Mesher.sampled(field,policy:policy)
    XCTAssertEqual(mesh.indices,again.indices);XCTAssertEqual(mesh.vertices.map(\.position),again.vertices.map(\.position))
  }
  func testImplicitCutSamplingPreservesSourceCoordinatesAndSkin() throws {
    let sections=[LoftSection(id:"back",z:-0.5,radius:SIMD2(0.28,0.25)),
      LoftSection(id:"middle",z:0,radius:SIMD2(0.35,0.32)),LoftSection(id:"front",z:0.5,radius:SIMD2(0.2,0.18))]
    let body=AnatomyElement(id:"envelope",region:"body",primitive:.loft,center:.zero,radius:V3(repeating:1),jointWeights:["root":1],sections:sections)
    let cut=AnatomyElement(id:"opening",region:"opening",operation:.cut,center:V3(0.3,0.05,0),radius:V3(0.15,0.13,0.2),cutBlend:0.025)
    let original=AnatomySource(id:"skin",part:"body",elements:[body,cut],resolution:24)
    var source=original;source.sampling=SurfaceSampling(baseEdgeLength:0.12,regions:[SurfaceSamplingRegion(id:"rim",center:cut.center,radius:V3(repeating:0.28),edgeLength:0.04)],maximumTetrahedra:100_000)
    XCTAssertFalse(source.changes(from:original).rebindRequired)
    let anchor=try original.anchor(id:"held",at:V3(-0.35,0,0))
    XCTAssertTrue(try AnatomyCompiler.rebind([anchor],from:original,to:source).accepted)
    let output=try AnatomyCompiler.compile(source,captureAnchors:false)
    XCTAssertNotNil(output.report.sampling);XCTAssertTrue(SurfaceCompiler.isClosed(welded(output.mesh)))
    XCTAssertGreaterThan(output.report.sampling!.recoveryPasses,0)
    XCTAssertEqual(output.report.sampling!.sampledInvertedTriangles,0)
    XCTAssertTrue(output.report.sampling!.orientedEdgeIncidence)
    XCTAssertEqual(output.joints,["root"]);XCTAssertTrue(output.weights.allSatisfy{$0.weights.x==1})
    var craft=CreatureCraft();craft.anatomy=[source]
    let data=try JSONEncoder().encode(craft);try CraftSchema.validate(JSONSerialization.jsonObject(with:data))
    XCTAssertEqual(try JSONDecoder().decode(CreatureCraft.self,from:data),craft)
  }
  func testCapsuleEnclosureConvergesAwayFromItsDiagonalAxis() throws {
    let field=Shape.capsule(V3(-1,-1,-1),V3(1,1,1),0.1),p=V3(0.8,-0.8,0)
    let box=Bounds(p-V3(repeating:0.001),p+V3(repeating:0.001)),interval=field.valueInterval(in:box)
    XCTAssertGreaterThan(interval.lower,1)
    XCTAssertLessThan(interval.upper-interval.lower,0.0035)
    for x in 0...2 {for y in 0...2 {for z in 0...2 {
      let q=box.min+(box.max-box.min)*V3(Float(x),Float(y),Float(z))/2,value=field.value(at:q)
      XCTAssertGreaterThanOrEqual(value,interval.lower);XCTAssertLessThanOrEqual(value,interval.upper)
    }}}
    let policy=SurfaceSampling(baseEdgeLength:0.08,maximumTetrahedra:100_000)
    let output=try Mesher.sampled(field,policy:policy)
    XCTAssertTrue(SurfaceCompiler.isClosed(welded(output)))
  }
  func testFineEndOfLongSourceDoesNotRefineTheFarBody() throws {
    let field=Shape.capsule(V3(0,0,-1.6),V3(0,0,1.6),0.22)
      .joined(.sphere(0.36).moved(V3(0,0,-1.65)))
    let bounds=field.bounds.expanded(0.03)
    let coarse=SurfaceSampling(baseEdgeLength:0.16,maximumTetrahedra:100_000)
    var detail=coarse
    detail.regions=[SurfaceSamplingRegion(id:"end",center:V3(0,0,-1.65),radius:V3(repeating:0.48),edgeLength:0.04)]
    let base=try AdaptiveTetrahedra.build(field,bounds:bounds,policy:coarse)
    let local=try AdaptiveTetrahedra.build(field,bounds:bounds,policy:detail)
    let farBase=base.cells.filter{$0.bounds.min.z>0.7}.count
    let farLocal=local.cells.filter{$0.bounds.min.z>0.7}.count
    XCTAssertGreaterThan(local.transitionFaces,0)
    XCTAssertGreaterThan(local.cells.count,base.cells.count*2)
    XCTAssertLessThanOrEqual(farLocal,farBase*2)
    let output=try Mesher.sampled(field,policy:detail)
    XCTAssertTrue(SurfaceCompiler.isClosed(welded(output)))
    XCTAssertEqual(SurfaceCompiler.sampledInvertedTriangles(output,field:field),0)
  }
  func testAuditWeldsAttributeSeamsAndKeepsSmallValidTriangles() {
    let scale:Float=0.000001,center=V3(repeating:scale/4),field=Shape.sphere(scale/2).moved(center)
    let points=[V3.zero,V3(scale,0,0),V3(0,scale,0),V3(0,0,scale)]
    var mesh=Mesh()
    for ids in [[0,2,1],[0,1,3],[0,3,2],[1,2,3]] {
      let vertices=ids.map{Vertex(points[$0],normalize(points[$0]-center),V3(repeating:Float(mesh.indices.count)))}
      mesh.triangle(vertices[0],vertices[1],vertices[2])
    }
    let good=SamplingGeometry.audit(mesh,field:field)
    XCTAssertTrue(good.orientedEdgeIncidence);XCTAssertEqual(good.degenerateTriangles,0);XCTAssertTrue(good.inwardSamples.isEmpty)
    var reversed=mesh
    for i in stride(from:0,to:reversed.indices.count,by:3) {reversed.indices.swapAt(i,i+1)}
    XCTAssertEqual(SamplingGeometry.audit(reversed,field:field).inwardSamples.count,4)
    mesh.triangle(mesh.vertices[0],mesh.vertices[0],mesh.vertices[1])
    let bad=SamplingGeometry.audit(mesh,field:field)
    XCTAssertEqual(bad.degenerateTriangles,1);XCTAssertFalse(bad.orientedEdgeIncidence)
    let cleaned=SamplingGeometry.removeZeroAreaTriangles(mesh)
    XCTAssertEqual(cleaned.removed,1);XCTAssertEqual(cleaned.mesh.indices.count,12)
    XCTAssertTrue(SamplingGeometry.audit(cleaned.mesh,field:field).orientedEdgeIncidence)
  }
  func testNearZeroCornersHaveOneCanonicalPosition() throws {
    let field=Shape.box(V3(1.1,2,2)).moved(V3(-1,0,0))
    let points=[V3(-1,0,0),V3(0.1,0,0),V3(0.3,1,0),V3(0.3,0,1)]
    var surface=TetrahedralSurface(field:field,color:V3(repeating:1),refine:false)
    surface.append(ids:[0,1,2,3],points:points,values:points.map{field.value(at:$0)})
    let mesh=surface.finish().mesh
    XCTAssertTrue(mesh.vertices.contains{V3($0.position.x,$0.position.y,$0.position.z)==points[1]})
    let near=Shape.sphere(Float(0.5).nextUp)
    let grid=try AdaptiveTetrahedra.build(near,bounds:Bounds(V3(repeating:-1),V3(repeating:1)),
      policy:SurfaceSampling(baseEdgeLength:0.25,maximumTetrahedra:10_000))
    XCTAssertGreaterThan(grid.zeroClassifiedVertices,0)
    XCTAssertLessThanOrEqual(grid.maximumLinearizedZeroDistance,4*Float(0.5).ulp)
    let corner=try XCTUnwrap(grid.points.firstIndex(of:V3(0.5,0,0)))
    XCTAssertEqual(grid.values[corner],0)
  }
  func testVolumeBudgetRejectsBeforeReturningPartialSurface() throws {
    let policy=SurfaceSampling(baseEdgeLength:0.012,maximumTetrahedra:1_000)
    XCTAssertThrowsError(try Mesher.sampled(.sphere(0.5),policy:policy)) {XCTAssertTrue($0.localizedDescription.contains("budget"))}
    var invalid=policy;invalid.regions=[SurfaceSamplingRegion(id:"bad",center:.zero,radius:V3(repeating:0.1),edgeLength:0.1)]
    XCTAssertThrowsError(try invalid.validate())
  }
}
