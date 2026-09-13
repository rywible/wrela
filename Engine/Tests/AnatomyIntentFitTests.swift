import XCTest
import FieldCore
import FieldCompiler
import simd

final class AnatomyIntentFitTests:XCTestCase {
  func testLinkedSourceChangesFitBothSidesWhileHoldingOtherAxes() throws {
    let source=AnatomySource(id:"form",part:"form",elements:[
      AnatomyElement(id:"left",region:"mass",center:V3(-0.4,0,0),radius:V3(repeating:0.25),blend:0,jointWeights:["root":1]),
      AnatomyElement(id:"right",region:"mass",center:V3(0.4,0,0),radius:V3(repeating:0.25),blend:0,jointWeights:["root":1])])
    let controls=[AnatomyFitControl(id:"width",terms:[AnatomyFitTerm(element:"left",parameter:"radius.x"),AnatomyFitTerm(element:"right",parameter:"radius.x")],minimum:-0.1,maximum:0.1)]
    let targets=[AnatomySurfaceTarget(id:"left",position:V3(-0.6,0,0),tolerance:0.0001),AnatomySurfaceTarget(id:"right",position:V3(0.6,0,0),tolerance:0.0001),AnatomySurfaceTarget(id:"top",position:V3(0.4,0.25,0),tolerance:0.0001)]
    let fit=try AnatomyIntentFit.fit(source,controls:controls,targets:targets)
    XCTAssertEqual(fit.changes[0].delta,-0.05,accuracy:0.0001)
    for i in 0..<2 {
      XCTAssertEqual(fit.source.elements[i].radius.x,0.2,accuracy:0.0001)
      XCTAssertEqual(fit.source.elements[i].radius.y,source.elements[i].radius.y)
      XCTAssertEqual(fit.source.elements[i].jointWeights,source.elements[i].jointWeights)
    }
    XCTAssertTrue(fit.residuals.allSatisfy{$0.linearizedDistance<=$0.tolerance})
    XCTAssertEqual(try AnatomyIntentFit.fit(source,controls:controls,targets:targets).source,fit.source)
  }
  func testConflictReportsTargetResidualAndSaturatedControlWithoutMutatingInput() throws {
    let source=AnatomySource(id:"form",part:"form",elements:[AnatomyElement(id:"mass",region:"mass",center:.zero,radius:V3(repeating:0.25))])
    let controls=[AnatomyFitControl(id:"bounded-width",terms:[AnatomyFitTerm(element:"mass",parameter:"radius.x")],minimum:-0.01,maximum:0.01)]
    XCTAssertThrowsError(try AnatomyIntentFit.fit(source,controls:controls,targets:[AnatomySurfaceTarget(id:"too-far",position:V3(0.35,0,0),tolerance:0.0001)])) {error in
      XCTAssertTrue(error.localizedDescription.contains("too-far"));XCTAssertTrue(error.localizedDescription.contains("bounded-width"));XCTAssertTrue(error.localizedDescription.contains("source unchanged"))
    }
    XCTAssertEqual(source.elements[0].radius,V3(repeating:0.25))
    var invalid=controls;invalid[0].terms[0].parameter=""
    XCTAssertThrowsError(try AnatomyIntentFit.fit(source,controls:invalid,targets:[AnatomySurfaceTarget(id:"t",position:V3(0.3,0,0))]))
  }
  func testCapsuleEndpointControlDoesNotChangeCrossSection() throws {
    let source=AnatomySource(id:"limb",part:"limb",elements:[AnatomyElement(id:"shaft",region:"limb",primitive:.capsule,center:V3(0,-0.3,0),radius:V3(repeating:0.1),end:V3(0,0.3,0))])
    let controls=[AnatomyFitControl(id:"length",terms:[AnatomyFitTerm(element:"shaft",parameter:"end.y"),AnatomyFitTerm(element:"shaft",parameter:"center.y",scale:-1)],minimum:-0.1,maximum:0.1)]
    let targets=[AnatomySurfaceTarget(id:"upper",position:V3(0,0.45,0)),AnatomySurfaceTarget(id:"lower",position:V3(0,-0.45,0)),AnatomySurfaceTarget(id:"side",position:V3(0.1,0,0))]
    let result=try AnatomyIntentFit.fit(source,controls:controls,targets:targets)
    XCTAssertEqual(result.source.elements[0].end.y,0.35,accuracy:0.001)
    XCTAssertEqual(result.source.elements[0].center.y,-0.35,accuracy:0.001)
    XCTAssertEqual(result.source.elements[0].radius,source.elements[0].radius)
  }
}
