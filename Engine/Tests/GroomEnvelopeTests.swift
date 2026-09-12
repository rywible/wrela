import XCTest
import FieldCore
import FieldCompiler
import simd

final class GroomEnvelopeTests: XCTestCase {
  func testLayeredCoverageHasIndependentPhaseAndRetainsGuideCorrespondence() throws {
    let chain=CraftGuideChain(id:"lock",joint:"head",points:[V3(0,2,0),V3(0,1,0),V3(0,0,0)])
    var design=GroomDesign(id:"hair",part:"hair",guides:[["lock-0","lock-1","lock-2"]],
      fibres:12,width:0.16,curl:0,envelope:GroomEnvelope(layers:3,clumps:6))
    let (mesh,skin,ids)=try CraftGroom.compile(design,rig:chain.rig)
    XCTAssertEqual(skin.count,mesh.vertices.count)
    XCTAssertTrue(skin.allSatisfy{$0.validate(count:ids.count)})
    let roots=mesh.vertices.filter{$0.position.y>1.999 && $0.groom.w>0}
    XCTAssertGreaterThan(Set(roots.map{$0.groom.z}).count,1)
    XCTAssertGreaterThan(roots.map{hypot($0.position.x,$0.position.z)}.max()!,
                         roots.map{hypot($0.position.x,$0.position.z)}.min()!*2)
    // Reconstruct the guide curve parameter from its two-node binding. Even a
    // straight Catmull-Rom guide eases near its repeated endpoint controls.
    // Shortening inner layers must also shorten their bindings.
    for (v,w) in zip(mesh.vertices,skin) {
      var parameter:Float=0
      for k in 0..<4 {parameter += Float(w.joints[k])*w.weights[k]/2}
      let expected=GuideGroom.point(chain.points,parameter).y
      XCTAssertEqual(v.position.y,expected,accuracy:0.004)
    }
    design.envelope=GroomEnvelope(layers:5)
    XCTAssertThrowsError(try CraftGroom.compile(design,rig:chain.rig))
    design.envelope=GroomEnvelope(clumps:9)
    XCTAssertThrowsError(try CraftGroom.compile(design,rig:chain.rig))
    let legacy=Data(#"{"coverage":0.96,"taper":0.42,"flatten":0.72,"ridge":0.045}"#.utf8)
    XCTAssertEqual(try JSONDecoder().decode(GroomEnvelope.self,from:legacy).layers,1)
    XCTAssertEqual(try JSONDecoder().decode(GroomEnvelope.self,from:legacy).clumps,1)
  }
  func testEnvelopeHasDenseTaperedMassAndDeterministicGuideBindings() throws {
    let chain = CraftGuideChain(id:"lock",joint:"head",points:[V3(0,2,0),V3(0,1,0),V3(0,0,0)])
    let design = GroomDesign(id:"hair",part:"hair",guides:[["lock-0","lock-1","lock-2"]],
      fibres:12,width:0.16,radius:0.0015,curl:0,envelope:GroomEnvelope())
    let (mesh,skin,ids) = try CraftGroom.compile(design,rig:chain.rig)
    let (again,againSkin,againIDs) = try CraftGroom.compile(design,rig:chain.rig)
    XCTAssertEqual(mesh.vertices.map(\.position),again.vertices.map(\.position))
    XCTAssertEqual(skin.map(\.joints),againSkin.map(\.joints))
    XCTAssertEqual(skin.map(\.weights),againSkin.map(\.weights)); XCTAssertEqual(ids,againIDs)
    XCTAssertEqual(mesh.vertices.count,skin.count)
    XCTAssertTrue(skin.allSatisfy { $0.validate(count:ids.count) })
    XCTAssertTrue(mesh.vertices.allSatisfy { CraftMath.finite(V3($0.position.x,$0.position.y,$0.position.z),limit:10)
      && CraftMath.finite(V3($0.normal.x,$0.normal.y,$0.normal.z),limit:1.001) })
    // Test physical coverage and taper independently of exact tessellation.
    let roots = mesh.vertices.filter { $0.position.y>1.97 }
    let tips = mesh.vertices.filter { $0.position.y<0.01 }
    func radial(_ vertices: [Vertex]) -> Float {
      vertices.map { hypot($0.position.x,$0.position.z) }.max() ?? 0
    }
    XCTAssertGreaterThan(radial(roots),0.11)
    XCTAssertLessThan(radial(tips),radial(roots)*0.25)
  }

  func testEnvelopeRejectsBadSourceAndKeepsLegacyDecoding() throws {
    XCTAssertThrowsError(try GroomEnvelope(flatten:0).validate(id:"broken"))
    XCTAssertThrowsError(try GroomEnvelope(coverage:.infinity).validate(id:"broken"))
    let legacy = GroomDesign(id:"old",part:"hair",guides:[["a","b"]])
    let decoded = try JSONDecoder().decode(GroomDesign.self,from:JSONEncoder().encode(legacy))
    XCTAssertNil(decoded.envelope)
    var volumetric = legacy; volumetric.envelope = GroomEnvelope(coverage:1.1,taper:0.6,flatten:0.5,ridge:0.02)
    XCTAssertEqual(try JSONDecoder().decode(GroomDesign.self,from:JSONEncoder().encode(volumetric)),volumetric)
  }
}
