import XCTest
import FieldCore
import FieldCompiler
import simd

final class AuthoringConstraintTests: XCTestCase {
  func testDenseSkinAuditDetectsVerticesBetweenClearGuides() {
    let mesh=ParametricMesh.ellipsoid(.zero,V3(repeating:0.1),detail:8)
    var left=matrix_identity_float4x4,right=matrix_identity_float4x4
    left.columns.3=SIMD4(-1,0,0,1);right.columns.3=SIMD4(1,0,0,1)
    let weights=mesh.vertices.map{_ in SkinWeight(0,1,blend:0.5)}
    let body=RigCapsule("body",joint:"root",a:V3(0,-1,0),b:V3(0,1,0),radius:0.5)
    let result=SkinContactAudit.evaluate(mesh:mesh,weights:weights,palette:[left,right],model:matrix_identity_float4x4,bodies:[body],height:{_,_ in -10})
    XCTAssertEqual(result.penetratingVertices,mesh.vertices.count)
    XCTAssertEqual(result.collider,"body");XCTAssertGreaterThan(result.maximumPenetration,0.39)
    var placement=matrix_identity_float4x4;placement.columns.3=SIMD4(2,0,0,1)
    let moved=SkinContactAudit.evaluate(mesh:mesh,weights:weights,palette:[left,right],model:placement,bodies:[body],height:{_,_ in -10})
    XCTAssertEqual(moved.maximumPenetration,0);XCTAssertEqual(moved.vertex,-1)
  }
  func testLocalFieldDerivativeAndUntouchedRegion() throws {
    let edit=SurfaceEdit(id:"cheek",part:"mask",center:.zero,radius:V3(1,2,1),offset:V3(0.1,0.2,-0.1),dilation:V3(0.1,-0.1,0))
    try edit.validate()
    XCTAssertEqual(edit.evaluate(V3(3,0,0)).position,V3(3,0,0))
    let p=V3(0.2,0.3,0.1),j=edit.evaluate(p).jacobian,e:Float=0.0005
    for axis in 0..<3 {
      var delta=V3.zero;delta[axis]=e
      let numerical=(edit.evaluate(p+delta).position-edit.evaluate(p-delta).position)/(2*e)
      XCTAssertLessThan(length(numerical-j[axis]),0.0002)
    }
    let mesh=ParametricMesh.ellipsoid(.zero,V3(repeating:0.5),detail:12)
    let result=try SurfaceEditing.apply([edit],to:mesh)
    XCTAssertEqual(mesh.indices,result.indices)
    XCTAssertNotEqual(mesh.vertices[0].position,result.vertices[0].position)
    for v in result.vertices {XCTAssertEqual(length(V3(v.normal.x,v.normal.y,v.normal.z)),1,accuracy:0.0001)}
    let miss=SurfaceEdit(id:"miss",part:"mask",center:V3(20,0,0),radius:V3(repeating:0.2))
    XCTAssertThrowsError(try SurfaceEditing.apply([miss],to:mesh))
  }
  func testSurfaceProjectionAndContinuousGridBindings() {
    func surface(_ u:Float,_ v:Float)->V3 {V3(u*2,0,v*3)}
    let uv=SurfaceSkinning.project(V3(0.7,0.3,1.8),position:surface)
    XCTAssertLessThan(length(uv-SIMD2<Float>(0.35,0.6)),0.0001)
    let w=SurfaceSkinning.weights(uv:uv,columns:4,rows:3)
    XCTAssertTrue(w.validate(count:12))
    var p=V3.zero
    for k in 0..<4 {let i=Int(w.joints[k]);p+=surface(Float(i%4)/3,Float(i/4)/2)*w.weights[k]}
    XCTAssertLessThan(length(p-V3(0.7,0,1.8)),0.0001)
    let end=SurfaceSkinning.weights(uv:SIMD2(repeating:1),columns:4,rows:3)
    XCTAssertEqual(end.joints.w,11);XCTAssertEqual(end.weights.w,1)
  }
  func testSurfaceAttachmentFollowsDifferentialNormal() {
    func surface(_ u:Float,_ v:Float)->V3 {V3(u,v,u*u)}
    let p=ParametricMesh.offsetPoint(u:0.5,v:0.3,distance:0.04,position:surface)
    let delta=p-surface(0.5,0.3)
    XCTAssertEqual(length(delta),0.04,accuracy:0.00001)
    XCTAssertEqual(dot(delta,normalize(V3(1,0,1))),0,accuracy:0.00002)
    XCTAssertGreaterThan(delta.z,0)
  }
  func testFinishBoundsAndGPURepresentation() throws {
    var layer=SurfaceLayer(id:"wear",part:"mask",center:V3(1,2,3),radius:V3(2,1,0.5),tint:V3(0.2,0.3,0.4))
    try layer.validate()
    XCTAssertEqual(layer.envelope(layer.center),1)
    XCTAssertEqual(layer.envelope(V3(3,2,3)),0)
    XCTAssertEqual(layer.envelope(V3(2,2,3)),0.5625,accuracy:0.00001)
    XCTAssertEqual(MemoryLayout<SurfaceLayerUniform>.stride,64)
    XCTAssertEqual(layer.uniform.shape,SIMD4<Float>(0.5,1,2,20))
    XCTAssertEqual(try JSONDecoder().decode(SurfaceLayer.self,from:JSONEncoder().encode(layer)),layer)
    layer.radius.x=0;XCTAssertThrowsError(try layer.validate())
    layer.radius.x=1;layer.pattern=4;XCTAssertThrowsError(try layer.validate())
  }
  func testGuideSculptPreservesBindingsAndRotatesNormals() {
    let mesh=ParametricMesh.surface(u:2,v:2,position:{u,v in V3(u,v,0)},tint:{_,_ in V3(repeating:1)})
    let weights=mesh.vertices.map{SkinWeight(0,1,blend:$0.position.x)}
    let sculpted=GuideDeformation.apply(mesh,weights:weights,joints:["left","right"],offsets:["right":V3(0,0,1)])
    XCTAssertEqual(sculpted.indices,mesh.indices)
    for i in mesh.vertices.indices {
      let p=mesh.vertices[i].position,q=sculpted.vertices[i].position,n=sculpted.vertices[i].normal
      XCTAssertEqual(q,SIMD4(p.x,p.y,p.x,p.w))
      XCTAssertEqual(abs(n.x),sqrt(0.5),accuracy:0.0001)
      XCTAssertEqual(abs(n.z),sqrt(0.5),accuracy:0.0001)
    }
  }
  func testHeightFieldUnderMovingActor() {
    var frame=simd_float4x4(simd_quatf(angle:.pi/2,axis:V3(0,1,0)))
    frame.columns.0 *= 2;frame.columns.1 *= 2;frame.columns.2 *= 2
    frame.columns.3=SIMD4(3,1,4,1)
    func world(_ x:Float,_ z:Float)->Float {0.2*x+0.1*z}
    func local(_ x:Float,_ z:Float)->Float {ContactRig.localHeight(frame:frame,x:x,z:z,height:world)}
    let p=frame*SIMD4<Float>(1,local(1,2),2,1)
    XCTAssertEqual(p.y,world(p.x,p.z),accuracy:0.00001)
    let n=ContactRig.surfaceNormal(x:1,z:2,height:local)
    XCTAssertEqual(n.x,normalize(V3(0.1,1,-0.2)).x,accuracy:0.0001)
    XCTAssertEqual(n.z,normalize(V3(0.1,1,-0.2)).z,accuracy:0.0001)
  }
  func testCubicTimingPreservesAuthoredVelocity() throws {
    var a=MotionKey(0,0),b=MotionKey(2,2);a.outTangent=1;b.inTangent=1
    let curve=MotionCurve([a,b]);try curve.validate(duration:2)
    XCTAssertEqual(curve.value(at:0.5),0.5,accuracy:0.00001)
    XCTAssertEqual(curve.value(at:1.5),1.5,accuracy:0.00001)
    b.inTangent = .infinity
    XCTAssertThrowsError(try MotionCurve([a,b]).validate(duration:2))
  }
  func testContactsAfterBodyCorrectionAndUnreachableDiagnostics() throws {
    let joints=[PartJoint(id:"body"),PartJoint(id:"upper",pivot:V3(0,2,0)),PartJoint(id:"lower",pivot:V3(0,1,0.6)),PartJoint(id:"paw",pivot:V3(0,0.18,0))]
    let chain=ContactChain(id:"leg",parent:"body",upper:"upper",lower:"lower",foot:"paw",pole:V3(0,0,2),sole:0.18)
    let pose=PartRig.matrices(joints,poses:["body":JointPose(offset:V3(0.2,0.15,0))])
    var surface=RehearsalSurface();surface.stepHeight=0.3;surface.stepZ=1
    let result=ContactRig.solve(joints:joints,matrices:pose,chains:[chain],targets:["leg":ContactTarget(V3(0,0.18,0),planted:true)],height:surface.height,normal:surface.normal)
    XCTAssertEqual(result.contacts.count,1)
    let c=result.contacts[0]
    XCTAssertEqual(c.actual.y,0.48,accuracy:0.0001)
    XCTAssertLessThan(c.residual,0.0001)
    XCTAssertLessThan(abs(c.clearance),0.0001)
    XCTAssertGreaterThan(c.correction,0.29)
    let upperEnd=result.matrices["upper"]!*SIMD4<Float>(0,1,0.6,1)
    let lowerStart=result.matrices["lower"]!*SIMD4<Float>(0,1,0.6,1)
    XCTAssertLessThan(length(upperEnd-lowerStart),0.0001)
    let impossible=ContactRig.solve(joints:joints,matrices:pose,chains:[chain],targets:["leg":ContactTarget(V3(8,0.18,0),planted:true)])
    XCTAssertGreaterThan(impossible.contacts[0].residual,5)
    XCTAssertEqual(try JSONDecoder().decode(RehearsalSurface.self,from:JSONEncoder().encode(surface)),surface)
  }
  func testCapsuleDistancesAndExcludedPairs() throws {
    XCTAssertEqual(RigCollision.segmentDistance(V3(0,0,0),V3(2,0,0),V3(1,-1,0),V3(1,1,0)),0,accuracy:0.0001)
    XCTAssertEqual(RigCollision.segmentDistance(.zero,V3(1,0,0),V3(0,2,0),V3(1,2,0)),2,accuracy:0.0001)
    XCTAssertEqual(RigCollision.segmentDistance(.zero,.zero,V3(3,0,0),V3(3,0,0)),3,accuracy:0.0001)
    let a=RigCapsule("a",joint:"body",a:V3(0,1,0),b:V3(1,1,0),radius:0.4)
    var b=RigCapsule("b",joint:"body",a:V3(0,1.2,0),b:V3(1,1.2,0),radius:0.4)
    XCTAssertLessThan(RigCollision.inspect([a,b],height:{_,_ in 0}).first(where:{$0.b=="b"})!.separation,0)
    b.ignore=["a"]
    XCTAssertEqual(RigCollision.inspect([a,b],height:{_,_ in 0}).count,2)
    b.radius = .nan;XCTAssertThrowsError(try b.validate())
  }

}
