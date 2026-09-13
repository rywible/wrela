import XCTest
import simd
@testable import FieldCore
@testable import FieldCompiler

final class SectionLoftTests:XCTestCase {
  func fixture()->SectionLoft {
    SectionLoft([LoftSection(id:"rear",z:-0.7,radius:SIMD2(0.3,0.45)),
      LoftSection(id:"wide",z:-0.2,center:SIMD2(0.03,0.1),radius:SIMD2(0.45,0.4)),
      LoftSection(id:"narrow",z:0.3,center:SIMD2(-0.02,0.02),radius:SIMD2(0.18,0.22)),
      LoftSection(id:"tip",z:0.7,radius:SIMD2(0.06,0.08))])
  }
  func testProfilesGradientsAndConservativeIntervals() throws {
    let l=fixture();try l.validate();let shape=Shape.loft(l)
    XCTAssertEqual(shape.semantics,.implicit)
    for s in l.sections {let v=l.profile(at:s.z).value;XCTAssertEqual(v.z,s.radius.x,accuracy:1e-6);XCTAssertEqual(v.y,s.center.y,accuracy:1e-6)}
    for i in 0..<80 {
      let z:Float = -0.69+Float(i)*1.38/80,v=l.profile(at:z).value
      let p=V3(v.x+v.z*0.8,v.y+v.w*0.6,z)
      XCTAssertEqual(shape.value(at:p),0,accuracy:1e-6)
      let g=shape.sample(at:p).gradient;var numerical=V3.zero
      for axis in 0..<3 {var d=V3.zero;d[axis]=0.0001;numerical[axis]=(shape.value(at:p+d)-shape.value(at:p-d))/0.0002}
      XCTAssertLessThan(length(g-numerical),0.002)
      XCTAssertLessThan(length(l.point(at:l.coordinate(at:p))-p),1e-6)
      let box=Bounds(p-V3(0.055,0.04,0.035),p+V3(0.04,0.065,0.055)),range=shape.valueInterval(in:box)
      for x in 0...2 {for y in 0...2 {for z in 0...2 {
        let q=box.min+(box.max-box.min)*V3(Float(x),Float(y),Float(z))/2,f=shape.value(at:q)
        XCTAssertGreaterThanOrEqual(f,range.lower-1e-6);XCTAssertLessThanOrEqual(f,range.upper+1e-6)
      }}}
    }
  }
  func testCompilationAndDurableSectionCoordinates() throws {
    let l=fixture(),shape=Shape.loft(l),mesh=try Mesher.compile(shape,resolution:32)
    XCTAssertGreaterThan(mesh.indices.count,1000)
    var geometric=mesh
    for i in geometric.vertices.indices {geometric.vertices[i].normal=SIMD4(0,1,0,0)}
    XCTAssertTrue(SurfaceCompiler.isClosed(MeshProcessing.optimize(geometric)))
    let e=AnatomyElement(id:"envelope",region:"body",primitive:.loft,center:V3(0,1,0),radius:V3(repeating:1),sections:l.sections)
    let source=AnatomySource(id:"study",part:"body",elements:[e]);try source.validate()
    let p=V3(0.2,1.2,0.12),q=e.coordinate(at:p)
    var changed=e;changed.sections![2].radius.x += 0.1
    let moved=changed.point(at:q)
    XCTAssertGreaterThan(moved.x,p.x)
    var next=source;next.elements=[changed]
    XCTAssertFalse(next.changes(from:source).rebindRequired)
    next.elements[0].sections!.swapAt(1,2)
    XCTAssertTrue(next.changes(from:source).rebindRequired)
    XCTAssertThrowsError(try next.validate())
    var subdivided=source
    let profile=l.profile(at:0.1).value
    subdivided.elements[0].sections!.insert(LoftSection(id:"inserted",z:0.1,center:SIMD2(profile.x,profile.y),radius:SIMD2(profile.z,profile.w)),at:2)
    let point=V3(0.16,1.02,0.3),anchor=try source.anchor(id:"held-section",at:point)
    let rejected=try AnatomyCompiler.rebind([anchor],from:source,to:subdivided)
    XCTAssertFalse(rejected.accepted)
    let recovered=try AnatomyCompiler.rebind([anchor],from:source,to:subdivided,policy:AnatomyRebindPolicy(allowStructuralChanges:true))
    XCTAssertTrue(recovered.accepted)
    XCTAssertLessThan(length(recovered.positions[0]-point),0.001)

    let data=try JSONEncoder().encode(source),json=try JSONSerialization.jsonObject(with:data)
    try CraftSchema.validate(json,definition:"anatomy")
    XCTAssertEqual(try JSONDecoder().decode(AnatomySource.self,from:data),source)
  }
  func testRemoteThinSectionDoesNotWeakenBroadSectionField() throws {
    let sections=[LoftSection(id:"wide",z:-0.5,radius:SIMD2(0.4,0.4)),LoftSection(id:"thin",z:0.5,radius:SIMD2(0.005,0.005))]
    let shape=Shape.loft(SectionLoft(sections))
    XCTAssertEqual(shape.value(at:V3(0.42,0,-0.5)),0.02,accuracy:0.000001)
    let p=V3(0,0,-0.2),step=V3(0,0,0.0001)
    XCTAssertEqual(shape.sample(at:p).gradient.z,(shape.value(at:p+step)-shape.value(at:p-step))/0.0002,accuracy:0.001)
  }
  func testSectionFitAndImplicitCollisionQueries() throws {
    let l=fixture(),element=AnatomyElement(id:"form",region:"form",primitive:.loft,center:.zero,radius:V3(repeating:1),sections:l.sections)
    let source=AnatomySource(id:"study",part:"body",elements:[element])
    let control=AnatomyFitControl(id:"width",terms:[AnatomyFitTerm(element:"form",parameter:"section.wide.radius.x")],minimum:-0.1,maximum:0.1)
    let targets=[AnatomySurfaceTarget(id:"side",position:V3(0.53,0.1,-0.2),tolerance:0.0001),AnatomySurfaceTarget(id:"top",position:V3(0.03,0.5,-0.2),tolerance:0.0001)]
    let result=try AnatomyIntentFit.fit(source,controls:[control],targets:targets)
    XCTAssertEqual(result.changes[0].delta,0.05,accuracy:0.0002)
    let explanation=try source.explain(at:V3(0.48,0.1,-0.2))
    XCTAssertTrue(explanation.contributors[0].controls.contains{$0.parameter=="section.wide.radius.x" && abs($0.normalResponse)>0.5})
    let shape=Shape.loft(l)
    XCTAssertFalse(FieldQueries.visible(from:V3(-1,0,0),to:V3(1,0,0),solid:shape))
    XCTAssertTrue(FieldQueries.visible(from:V3(-1,2,0),to:V3(1,2,0),solid:shape))
    let distance=shape.conservativeDistance(at:V3(0.6,0.1,-0.2))
    XCTAssertGreaterThan(distance,0);XCTAssertLessThanOrEqual(distance,0.12001)
    XCTAssertEqual(shape.blended(.sphere(0.1),radius:0.02).semantics,.implicit)
    let wall=Shape.loft(SectionLoft([LoftSection(id:"a",z:-0.6,radius:SIMD2(0.08,2)),LoftSection(id:"b",z:0.6,radius:SIMD2(0.08,2))]))
    let stopped=FieldQueries.moveCapsule(eye:V3(-0.6,1.7,0),delta:V3(1.2,0,0),solid:wall)
    XCTAssertLessThan(stopped.x,-0.25);XCTAssertGreaterThan(stopped.x,-0.61)
    XCTAssertTrue(stopped.y.isFinite && stopped.z.isFinite)

  }

}
