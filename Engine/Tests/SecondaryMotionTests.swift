import XCTest
import FieldCore
import FieldCompiler
import simd

final class SecondaryMotionTests:XCTestCase {
  func testSurfaceGuideFrameReproducesStretchShearAndThickness() throws {
    let old=[V3(1,0,0),V3(-1,0,0),V3(0,0,1),V3(0,0,-1)]
    let affine=simd_float3x3(columns:(V3(2,0,0),V3(0,1,0),V3(0.3,0,0.5)))
    let frame=try XCTUnwrap(GuideFrame.surface(old:old,new:old.map{affine*$0}))
    for p in [V3(0.3,0,0.7),V3(0,0.1,0)] {XCTAssertLessThan(length(frame*p-affine*p),0.00001)}
    XCTAssertNil(GuideFrame.surface(old:[V3(1,0,0),V3(2,0,0)],new:[V3(1,0,0),V3(2,0,0)]))
    // Opposite neighbours folded onto one side cancel the fitted tangent.
    XCTAssertNil(GuideFrame.surface(old:old,new:[V3(1,0,0),V3(1,0,0),V3(0,0,1),V3(0,0,-1)]))
  }
  func testTaperedContactKeepsNarrowAttachmentAndFindsWideTip() {
    let a=V3(0,0.2,0),b=V3(1,0.3,0),c=V3(0,0,-1),d=V3(0,0,1)
    let uv=RigCollision.taperedParameters(a,b,c,d,radiusA:0.02,radiusB:0.3)
    let clearance=length(a+(b-a)*uv.x-c-(d-c)*uv.y)-(0.02+0.28*uv.x)
    // A constant .3 m tube would report penetration at the .02 m root.
    XCTAssertGreaterThan(clearance,0.17)
    var minimum=Float.greatestFiniteMagnitude
    for i in 0...10000 {let u=Float(i)/10000,p=a+(b-a)*u;minimum=min(minimum,RigCollision.segmentDistance(p,p,c,d)-(0.02+0.28*u))}
    XCTAssertEqual(clearance,minimum,accuracy:0.00001)
    let reversed=RigCollision.taperedParameters(b,a,c,d,radiusA:0.3,radiusB:0.02)
    XCTAssertEqual(reversed.x,1-uv.x,accuracy:0.00001)
    let collinear=RigCollision.taperedParameters(.zero,V3(2,0,0),V3(-1,0,0),V3(3,0,0),radiusA:0.02,radiusB:0.3)
    XCTAssertEqual(collinear.x,1,accuracy:0.00001)
    let partial=RigCollision.taperedParameters(.zero,V3(2,0,0),V3(-1,0,0),V3(1.2,0,0),radiusA:0.02,radiusB:0.3)
    XCTAssertEqual(partial.x,0.6,accuracy:0.00001)
    let narrower=RigCollision.taperedParameters(.zero,V3(2,0,0),V3(-1,0,0),V3(3,0,0),radiusA:0.3,radiusB:0.02)
    XCTAssertEqual(narrower.x,0,accuracy:0.00001)
  }
  func testSpanContactFindsCollisionBetweenClearEndpoints() throws {
    let rig=SecondaryRig(nodes:[SecondaryNode("a",joint:"body",position:V3(-1,0,0),inverseMass:1,radius:0.05),SecondaryNode("b",joint:"body",position:V3(1,0,0),inverseMass:1,radius:0.05)],links:[SecondaryLink(0,1,contact:true)])
    let body=RigCapsule("post",joint:"body",a:V3(0,-1,0),b:V3(0,1,0),radius:0.2)
    let targets=rig.nodes.map(\.position);var state=SecondaryState(targets)
    let before=SecondaryMotion.metrics(state,rig:rig,targets:targets,bodies:[body],height:{_,_ in -10})
    XCTAssertEqual(before.maximumPenetration,0)
    XCTAssertEqual(before.maximumEdgePenetration,0.25,accuracy:1e-6)
    XCTAssertEqual(before.worstEdgePenetration,"a → b")
    var settings=SecondarySettings();settings.gravity=0;settings.wind=0;settings.edgeContacts=true
    SecondaryMotion.step(&state,rig:rig,settings:settings,from:targets,to:targets,bodies:[body],height:{_,_ in -10},time:0)
    XCTAssertLessThan(SecondaryMotion.metrics(state,rig:rig,targets:targets,bodies:[body],height:{_,_ in -10}).maximumEdgePenetration,0.0001)
    XCTAssertEqual(length(state.positions[0]-state.positions[1]),2,accuracy:0.0001)
  }
  func testFrictionDissipatesSlidingAndFollowsMovingSurface() {
    let rig=SecondaryRig(nodes:[SecondaryNode("p",joint:"body",position:V3(0,0.05,0),inverseMass:1,radius:0.05)])
    var settings=SecondarySettings();settings.wind=0;settings.damping=0;settings.followCompliance=5
    func slide(_ friction:Float)->Float {
      var s=settings;s.friction=friction;var state=SecondaryState(rig.nodes.map(\.position));state.velocities[0]=V3(2,0,0)
      for i in 0..<30 {SecondaryMotion.step(&state,rig:rig,settings:s,from:rig.nodes.map(\.position),to:rig.nodes.map(\.position),bodies:[],height:{_,_ in 0},time:Float(i)/60)}
      return state.positions[0].x
    }
    XCTAssertGreaterThan(slide(0),0.9);XCTAssertLessThan(slide(1),0.3)
    var movingRig=rig;movingRig.nodes[0].position=V3(0,0.25,0)
    var state=SecondaryState(movingRig.nodes.map(\.position));settings.friction=1
    func body(_ t:Float)->RigCapsule {RigCapsule("belt",joint:"body",a:V3(0,0,-2+t*0.1),b:V3(0,0,2+t*0.1),radius:0.2)}
    for i in 0..<60 {
      SecondaryMotion.step(&state,rig:movingRig,settings:settings,from:movingRig.nodes.map(\.position),to:movingRig.nodes.map(\.position),bodies:[body(Float(i+1)/60)],height:{_,_ in -10},time:Float(i+1)/60,previousBodies:[body(Float(i)/60)])
    }
    XCTAssertGreaterThan(state.positions[0].z,0.08)
    XCTAssertEqual(state.positions[0].y,0.25,accuracy:0.001)
  }
  func testPinnedPenetrationIsReportedAndOldSettingsDecode() throws {
    let rig=SecondaryRig(nodes:[SecondaryNode("buried-root",joint:"body",position:.zero,inverseMass:0)])
    let m=SecondaryMotion.metrics(SecondaryState([.zero]),rig:rig,targets:[.zero],bodies:[],height:{_,_ in 0})
    XCTAssertEqual(m.pinnedPenetration,0.025,accuracy:1e-6);XCTAssertEqual(m.worstPinnedPenetration,"buried-root")
    let old=Data(#"{"enabled":true,"gravity":9.81,"damping":5,"followCompliance":0.6,"wind":0.5,"substeps":4,"iterations":4}"#.utf8)
    var settings=try JSONDecoder().decode(SecondarySettings.self,from:old)
    XCTAssertFalse(settings.edgeContacts);XCTAssertEqual(settings.friction,0)
    settings.friction = -.infinity;XCTAssertThrowsError(try settings.validate())
    let link=try JSONDecoder().decode(SecondaryLink.self,from:Data(#"{"a":0,"b":1,"compliance":0.001}"#.utf8))
    XCTAssertFalse(link.contact)
  }
  func testPinnedChainAndSerializedContinuation() throws {
    let rig=SecondaryRig(nodes:[SecondaryNode("root",joint:"body",position:V3(0,2,0),inverseMass:0),SecondaryNode("tip",joint:"body",position:V3(0,1,0),inverseMass:1)],links:[SecondaryLink(0,1,compliance:0.0001)])
    try rig.validate()
    var settings=SecondarySettings();settings.wind=0;settings.followCompliance=5
    let targets=rig.nodes.map(\.position)
    var a=SecondaryState(targets)
    for i in 0..<120 {SecondaryMotion.step(&a,rig:rig,settings:settings,from:targets,to:targets,bodies:[],height:{_,_ in -10},time:Float(i)/60)}
    XCTAssertEqual(a.positions[0],targets[0])
    XCTAssertEqual(length(a.positions[1]-a.positions[0]),1.00098,accuracy:0.003)
    var b=try JSONDecoder().decode(SecondaryState.self,from:JSONEncoder().encode(a))
    for i in 120..<180 {
      SecondaryMotion.step(&a,rig:rig,settings:settings,from:targets,to:targets,bodies:[],height:{_,_ in -10},time:Float(i)/60)
      SecondaryMotion.step(&b,rig:rig,settings:settings,from:targets,to:targets,bodies:[],height:{_,_ in -10},time:Float(i)/60)
    }
    XCTAssertEqual(a,b)
  }
  func testCollisionResponseAndInvalidSource() throws {
    let rig=SecondaryRig(nodes:[SecondaryNode("p",joint:"body",position:V3(0,0.4,0),inverseMass:1,radius:0.05)])
    var settings=SecondarySettings();settings.wind=0;settings.followCompliance=5
    var state=SecondaryState([V3(0,0.4,0)])
    let body=RigCapsule("test",joint:"body",a:V3(-1,0.2,0),b:V3(1,0.2,0),radius:0.2)
    for i in 0..<240 {SecondaryMotion.step(&state,rig:rig,settings:settings,from:rig.nodes.map(\.position),to:rig.nodes.map(\.position),bodies:[body],height:{_,_ in 0},time:Float(i)/60)}
    XCTAssertGreaterThanOrEqual(state.positions[0].y,0.4499)
    XCTAssertLessThan(SecondaryMotion.metrics(state,rig:rig,targets:rig.nodes.map(\.position),bodies:[body],height:{_,_ in 0}).maximumPenetration,0.0001)
    settings.damping = .nan;XCTAssertThrowsError(try settings.validate())
    var bad=rig;bad.links=[SecondaryLink(0,2)];XCTAssertThrowsError(try bad.validate())
  }
  func testGroomBindingsAreDeterministicAndNormalized() {
    let guide=GroomGuide(points:[V3(0,2,0),V3(0,1,0),V3(0,0,0.2)],ids:["a","b","c"])
    let a=GuideGroom.compile([guide],fibres:5,color:V3(repeating:1))
    let b=GuideGroom.compile([guide],fibres:5,color:V3(repeating:1))
    XCTAssertEqual(a.mesh.indices,b.mesh.indices)
    XCTAssertEqual(a.mesh.vertices.map(\.position),b.mesh.vertices.map(\.position))
    XCTAssertEqual(a.weights.count,a.mesh.vertices.count)
    XCTAssertTrue(a.weights.allSatisfy{$0.validate(count:3)})
  }
}
