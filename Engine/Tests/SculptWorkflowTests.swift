import XCTest
import FieldCore
import FieldCompiler
import simd

final class SculptWorkflowTests:XCTestCase {
  func testPlaneBrushesProtectionAndEllipticalFalloff() throws {
    var stroke=SculptStroke(id:"plane",part:"face",brush:.scrape,points:[.zero],radius:1,strength:0.4,direction:V3(0,0,1))
    stroke.profile=SculptProfile(tangent:V3(1,0,0),radii:V3(1,0.3,0.2),hardness:0.5,planeOffset:0.02,
      protect:[SculptProtection(id:"eye",center:V3(0.4,0,0),radius:V3(repeating:0.1))])
    try stroke.validate()
    XCTAssertLessThan(stroke.field(V3(0,0,0.1)).position.z,0.1)
    XCTAssertEqual(stroke.field(V3(0,0,-0.1)).position,V3(0,0,-0.1))
    XCTAssertEqual(stroke.field(V3(0.4,0,0.01)).position,V3(0.4,0,0.01))
    XCTAssertEqual(stroke.coverage(V3(0,0.4,0)),0)
    XCTAssertGreaterThan(stroke.coverage(V3(0.4,0.1,0)),0)
    stroke.brush = .fill
    XCTAssertGreaterThan(stroke.field(V3(0,0,-0.1)).position.z,-0.1)
    XCTAssertEqual(stroke.field(V3(0,0,0.1)).position,V3(0,0,0.1))
    stroke.brush = .pinch
    XCTAssertLessThan(stroke.field(V3(0.2,0,0.05)).position.x,0.2)
    XCTAssertEqual(stroke.field(V3(0.2,0,0.05)).position.z,0.05)
    stroke.profile!.tangent=V3(0,0,1)
    XCTAssertThrowsError(try stroke.validate())
  }
  func testProfileMirrorsItsProtectionAndJacobian() throws {
    var s=SculptStroke(id:"cheek",part:"face",brush:.clay,points:[V3(0.5,0,0)],radius:0.4,strength:0.2,direction:V3(1,0,1),mirrorX:true)
    s.profile=SculptProfile(tangent:V3(0,1,0),radii:V3(0.3,0.4,0.2),planeOffset:0.02)
    let p=V3(0.55,0.1,0.02),a=s.field(p),b=s.field(V3(-p.x,p.y,p.z))
    XCTAssertLessThan(length(a.position-V3(-b.position.x,b.position.y,b.position.z)),0.00001)
    let e:Float=0.00005
    for i in 0..<3 {var d=V3.zero;d[i]=e;XCTAssertLessThan(length((s.field(p+d).position-s.field(p-d).position)/(2*e)-a.jacobian[i]),0.002)}
  }
  func testLayerVisibilityIntensityAndRoundTrip() throws {
    let mesh=ParametricMesh.ellipsoid(.zero,V3(repeating:1),detail:24)
    let s=SculptStroke(id:"move",part:"face",brush:.grab,points:[V3(0,1,0)],radius:0.4,strength:0.08)
    var layer=SculptLayer(id:"brow",opacity:0.5,strokes:[s])
    let edited=try SculptCompiler.apply(layer.activeStrokes,to:mesh)
    XCTAssertEqual(edited.vertices.map{$0.position.y}.max()!,1.04,accuracy:0.00001)
    layer.enabled=false
    let off=try SculptCompiler.apply(layer.activeStrokes,to:mesh)
    XCTAssertEqual(off.vertices.map(\.position),mesh.vertices.map(\.position))
    var craft=CreatureCraft();craft.sculptLayers=[layer]
    XCTAssertEqual(try JSONDecoder().decode(CreatureCraft.self,from:JSONEncoder().encode(craft)),craft)
  }
  func testRayUsesVisibleTriangleAndBarycentricBindCorrespondence() throws {
    let rest=ParametricMesh.surface(u:1,v:1,position:{u,v in V3(u,v,0)},tint:{_,_ in V3(repeating:1)})
    var m=matrix_identity_float4x4;m.columns.3=SIMD4(2,3,4,1)
    let posed=DeformationAudit.deformed(rest,weights:[],palette:[],rigid:m)
    let h=try XCTUnwrap(SurfacePicking.hit(rest:rest,posed:posed,origin:V3(2.23,3.41,6),direction:V3(0,0,-1)))
    XCTAssertLessThan(length(h.bindPosition-V3(0.23,0.41,0)),0.00001)
    XCTAssertLessThan(length(h.position-V3(2.23,3.41,4)),0.00001)
    XCTAssertNil(SurfacePicking.hit(rest:rest,posed:posed,origin:V3(4,3,6),direction:V3(0,0,-1)))
    let ray=try SurfacePicking.ray(pixel:SIMD2(49.5,49.5),size:SIMD2(100,100),inverseVP:matrix_identity_float4x4)
    XCTAssertEqual(ray.origin,.zero);XCTAssertEqual(ray.direction,V3(0,0,1))
    XCTAssertThrowsError(try SurfacePicking.ray(pixel:SIMD2(-1,0),size:SIMD2(100,100),inverseVP:matrix_identity_float4x4))
  }
  func testIntentFitsMovementAndPreservesAnchorsOnActualTriangles() throws {
    let mesh=ParametricMesh.surface(u:24,v:24,position:{u,v in V3(u*2-1,0,v*2-1)},tint:{_,_ in V3(repeating:1)})
    let controls=[SculptControl(center:.zero,radius:0.65),SculptControl(center:V3(-0.4,0,0),radius:0.5),SculptControl(center:V3(0.4,0,0),radius:0.5)]
    let targets=[SculptTarget(id:"cheek",point:.zero,target:V3(0,0.04,0)),SculptTarget(id:"eye",point:V3(-0.4,0,0),target:V3(-0.4,0,0),tolerance:0.0001),SculptTarget(id:"muzzle",point:V3(0.4,0,0),target:V3(0.4,0,0),tolerance:0.0001)]
    let result=try SurfaceIntentFit.fit(id:"form",part:"face",mesh:mesh,controls:controls,targets:targets)
    XCTAssertTrue(result.residuals.allSatisfy{$0.error<=$0.tolerance})
    let deformed=try SculptCompiler.apply(result.layer.activeStrokes,to:mesh)
    let center=mesh.vertices.indices.min{length(mesh.vertices[$0].position-SIMD4<Float>(0,0,0,1))<length(mesh.vertices[$1].position-SIMD4<Float>(0,0,0,1))}!
    XCTAssertEqual(deformed.vertices[center].position.y,0.04,accuracy:0.001)
    let conflict=targets+[SculptTarget(id:"hold-cheek",point:.zero,target:.zero,tolerance:0.0001)]
    XCTAssertThrowsError(try SurfaceIntentFit.fit(id:"conflict",part:"face",mesh:mesh,controls:controls,targets:conflict)) {error in
      XCTAssertTrue(error.localizedDescription.contains("conflict"));XCTAssertTrue(error.localizedDescription.contains("residual"))
    }
  }

  func testIntentProtectsARegionAndRejectsMovingItsInterior() throws {
    let mesh=ParametricMesh.surface(u:40,v:40,position:{u,v in V3(u*2-1,0,v*2-1)},tint:{_,_ in V3(repeating:1)})
    let protect=[SculptProtection(id:"eyelid",center:V3(0.3,0,0),radius:V3(0.22,0.15,0.3))]
    let controls=[SculptControl(center:.zero,radius:0.7)]
    let target=SculptTarget(id:"cheek",point:.zero,target:V3(0,0.035,0),tolerance:0.0001)
    let result=try SurfaceIntentFit.fit(id:"region",part:"face",mesh:mesh,controls:controls,targets:[target],protect:protect)
    XCTAssertGreaterThan(result.protectedVertices,30)
    XCTAssertEqual(result.maximumProtectedDisplacement,0)
    XCTAssertGreaterThan(result.maximumDisplacement,0.034)
    XCTAssertTrue(result.maximumNormalChangeDegrees.isFinite)
    XCTAssertThrowsError(try SurfaceIntentFit.fit(id:"distortion",part:"face",mesh:mesh,controls:controls,targets:[target],protect:protect,maximumNormalChangeDegrees:1))
    XCTAssertThrowsError(try SurfaceIntentFit.fit(id:"conflict",part:"face",mesh:mesh,controls:controls,
      targets:[SculptTarget(id:"protected-eye",point:V3(0.3,0,0),target:V3(0.3,0.01,0))],protect:protect))
    let broad=SculptProtection(id:"wide-feather",center:.zero,radius:V3(repeating:1),innerFraction:0.35)
    XCTAssertEqual(broad.retention(V3(0.3,0,0)),0)
    XCTAssertGreaterThan(broad.retention(V3(0.5,0,0)),0)
    XCTAssertLessThan(broad.retention(V3(0.5,0,0)),0.5)
    XCTAssertThrowsError(try SculptProtection(id:"invalid",center:.zero,radius:V3(repeating:1),innerFraction:1).validate())
  }

  func testOptionalAnatomyAnchorsDoNotChangeCompiledRepresentation() throws {
    let source=AnatomySource(id:"source",part:"body",elements:[AnatomyElement(id:"mass",region:"thorax",center:.zero,radius:V3(0.4,0.5,0.3))],resolution:24)
    let full=try AnatomyCompiler.compile(source),lean=try AnatomyCompiler.compile(source,captureAnchors:false)
    XCTAssertEqual(full.mesh.indices,lean.mesh.indices)
    XCTAssertEqual(full.mesh.vertices.map(\.position),lean.mesh.vertices.map(\.position))
    XCTAssertEqual(full.weights.map(\.weights),lean.weights.map(\.weights))
    XCTAssertEqual(full.anchors.count,full.mesh.vertices.count)
    XCTAssertTrue(lean.anchors.isEmpty)
  }

}
