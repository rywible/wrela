import XCTest
@testable import FieldCore
import simd

final class SecondaryPatchContactTests:XCTestCase {
  private func quad()->SecondaryRig {
    let positions=[V3(-1,1,-1),V3(1,1,-1),V3(-1,1,1),V3(1,1,1)]
    return SecondaryRig(nodes:positions.enumerated().map{SecondaryNode("q-\($0.offset)",joint:"body",position:$0.element,inverseMass:1,radius:0.015)},
      links:[SecondaryLink(0,1,contact:true),SecondaryLink(1,3,contact:true),SecondaryLink(3,2,contact:true),SecondaryLink(2,0,contact:true)],
      patches:[SecondaryPatch("panel",0,1,2,3)])
  }
  func testThinCapsuleContactsQuadInteriorWithClearEdgesAndVertices() throws {
    let rig=quad(),targets=rig.nodes.map(\.position)
    try rig.validate()
    let body=RigCapsule("thin-limb",joint:"body",a:V3(0.17,1.03,-0.2),b:V3(0.17,1.03,0.3),radius:0.08)
    var state=SecondaryState(targets)
    let before=SecondaryMotion.metrics(state,rig:rig,targets:targets,bodies:[body],height:{_,_ in -10})
    XCTAssertEqual(before.maximumPenetration,0)
    XCTAssertEqual(before.maximumEdgePenetration,0)
    XCTAssertEqual(before.maximumPatchPenetration,0.065,accuracy:0.0001)
    XCTAssertEqual(before.worstPatchPenetration,"panel → thin-limb")
    var settings=SecondarySettings();settings.wind=0;settings.gravity=0;settings.edgeContacts=true;settings.iterations=8;settings.followCompliance=5
    SecondaryMotion.step(&state,program:SecondaryProgram(rig:rig,settings:settings),from:targets,to:targets,bodies:[body],height:{_,_ in -10},time:0)
    let after=SecondaryMotion.metrics(state,rig:rig,targets:targets,bodies:[body],height:{_,_ in -10})
    XCTAssertLessThan(after.maximumPatchPenetration,0.001)
    XCTAssertGreaterThan(length(state.positions[0]-targets[0]),0.01)
  }
  func testInteriorSearchFindsSmallContactBetweenFixedSamplePositions() {
    var rig=quad()
    for i in rig.nodes.indices {rig.nodes[i].radius=0.001}
    let body=RigCapsule("needle",joint:"body",a:V3(0.173,1,0.217),b:V3(0.173,1,0.217),radius:0.005)
    var positions=rig.nodes.map(\.position)
    let hit=SecondaryContact.patchPenetration(positions,patch:rig.patches[0],rig:rig,bodies:[body],height:{_,_ in -10})
    XCTAssertEqual(hit.depth,0.006,accuracy:0.00001)
    let original=positions
    for _ in 0..<6 {SecondaryContact.patch(&positions,old:original,patch:rig.patches[0],rig:rig,before:[body],after:[body],height:{_,_ in -10},friction:0)}
    XCTAssertLessThan(SecondaryContact.patchPenetration(positions,patch:rig.patches[0],rig:rig,bodies:[body],height:{_,_ in -10}).depth,0.00001)
  }
  func testFourWeightCorrectionUsesSquaredGradientMassAndPreservesPins() {
    var rig=quad();rig.nodes[0].inverseMass=0;rig.nodes[1].inverseMass=1;rig.nodes[2].inverseMass=2;rig.nodes[3].inverseMass=4
    let original=rig.nodes.map(\.position),weights=rig.patches[0].weights(SIMD2(0.3,0.7)),delta=V3(0.1,0.2,-0.15)
    var positions=original
    SecondaryContact.applyPatch(&positions,patch:rig.patches[0],rig:rig,weights:weights,correction:delta)
    XCTAssertEqual(positions[0],original[0])
    var actual=V3.zero
    for k in 0..<4 {actual += (positions[k]-original[k])*weights[k]}
    XCTAssertLessThan(length(actual-delta),0.000001)
    XCTAssertEqual(length(positions[3]-original[3])/length(positions[1]-original[1]),4*weights.w/weights.y,accuracy:0.00001)
  }
  func testPinnedPatchResidualIsReportedAndInvalidSourceRejected() throws {
    var rig=quad()
    for i in rig.nodes.indices {rig.nodes[i].inverseMass=0}
    let body=RigCapsule("limb",joint:"body",a:V3(0,1.03,0),b:V3(0,1.03,0),radius:0.08)
    let points=rig.nodes.map(\.position);var state=SecondaryState(points)
    var settings=SecondarySettings();settings.edgeContacts=true
    SecondaryMotion.step(&state,rig:rig,settings:settings,from:points,to:points,bodies:[body],height:{_,_ in -10},time:0)
    XCTAssertEqual(state.positions,points)
    let metrics=SecondaryMotion.metrics(state,rig:rig,targets:points,bodies:[body],height:{_,_ in -10})
    XCTAssertEqual(metrics.pinnedPatchPenetration,0.065,accuracy:0.0001)
    XCTAssertEqual(metrics.worstPinnedPatchPenetration,"panel → limb")
    var invalid=rig;invalid.patches[0].nodes.w=invalid.patches[0].nodes.z
    XCTAssertThrowsError(try invalid.validate())
    invalid=rig;invalid.patches[0].nodes.w=99
    XCTAssertThrowsError(try invalid.validate())
    invalid=rig;invalid.patches.append(invalid.patches[0])
    XCTAssertThrowsError(try invalid.validate())
  }
  func testOldRigDecodesAndAppendedClothPatchesKeepNodeCorrespondence() throws {
    let old=Data(#"{"nodes":[],"links":[]}"#.utf8)
    XCTAssertTrue(try JSONDecoder().decode(SecondaryRig.self,from:old).patches.isEmpty)
    let first=quad()
    let panel=ClothPanel(id:"cloth",part:"cape",columns:2,rows:2,points:first.nodes.map(\.position),attachments:Array(repeating:"body",count:4),pins:[0])
    var combined=first;combined.append(panel.rig)
    try combined.validate()
    XCTAssertEqual(combined.patches.count,2)
    XCTAssertEqual(combined.patches[1].nodes,SIMD4(4,5,6,7))
    XCTAssertEqual(try JSONDecoder().decode(SecondaryRig.self,from:JSONEncoder().encode(combined)),combined)
  }
}
