import XCTest
import FieldCore
import FieldCompiler
import simd

final class AnatomySourceTests:XCTestCase {
  private func body()->AnatomySource {
    AnatomySource(id:"study-body",part:"body",elements:[
      AnatomyElement(id:"trunk",region:"thorax",center:.zero,radius:V3(0.45,0.8,0.3),jointWeights:["spine":1]),
      AnatomyElement(id:"shoulder",region:"shoulder",center:V3(0.35,0.3,0),radius:V3(0.32,0.25,0.27),rotation:V3(0,0,-25),blend:0.08,jointWeights:["spine":0.25,"arm":0.75]),
      AnatomyElement(id:"bone",region:"sternum",role:.internalStructure,center:.zero,radius:V3(0.1,0.4,0.1),jointWeights:["spine":1])
    ],resolution:24)
  }
  func testOrientedFieldAnalyticGradientAndConservativeBounds() throws {
    let q=simd_normalize(simd_quatf(angle:0.72,axis:normalize(V3(1,2,3))))
    let shape=Shape.sphere(1).stretched(V3(0.2,0.8,0.3)).rotated(q).moved(V3(0.5,0.3,-0.1))
    try shape.validate()
    var random=SeededRandom(seed:19)
    for _ in 0..<100 {
      let p=V3(random.range(-1,1),random.range(-1,1),random.range(-1,1)),sample=shape.sample(at:p),epsilon:Float=0.0001
      var gradient=V3.zero
      for axis in 0..<3 {var d=V3.zero;d[axis]=epsilon;gradient[axis]=(shape.value(at:p+d)-shape.value(at:p-d))/(2*epsilon)}
      XCTAssertLessThan(length(gradient-sample.gradient),0.003)
      let surface=q.act(normalize(p)*V3(0.2,0.8,0.3))+V3(0.5,0.3,-0.1)
      XCTAssertTrue(shape.bounds.expanded(0.000001).contains(surface))
      let region=Bounds(p-V3(repeating:0.05),p+V3(repeating:0.05))
      XCTAssertEqual(shape.specialized(in:region).value(at:p),sample.value,accuracy:0.000001)
    }
    XCTAssertTrue(try MetalField.source(for:shape).contains("float3x3"))
    XCTAssertThrowsError(try Shape.rotated(.sphere(1),simd_quatf(vector:SIMD4(0,0,0,2))).validate())
  }
  func testCutsChangeActualFieldAndInternalsDoNotInflateSkin() throws {
    var source=body(),without=source
    without.elements.removeAll{$0.role == .internalStructure}
    XCTAssertEqual(try source.shape().value(at:V3(0.1,0,0)),try without.shape().value(at:V3(0.1,0,0)),accuracy:0.000001)
    source.elements.append(AnatomyElement(id:"socket",region:"socket",operation:.cut,center:V3(0,0.5,-0.27),radius:V3(repeating:0.18),color:V3(0.3,0.2,0.1)))
    try source.validate(joints:["spine","arm"])
    XCTAssertLessThan(try without.shape().value(at:V3(0,0.5,-0.20)),0)
    XCTAssertGreaterThan(try source.shape().value(at:V3(0,0.5,-0.20)),0)
    let sample=source.sample(at:V3(0,0.5,-0.20))
    XCTAssertEqual(sample.region,"socket")
    XCTAssertEqual(sample.jointWeights.values.reduce(0,+),1,accuracy:0.00001)
    for p in [V3.zero,V3(0.4,0.4,0.1),V3(0,0.5,-0.20)] {
      XCTAssertEqual(source.sample(at:p).field.value,try source.shape().value(at:p),accuracy:0.000001)
    }
  }
  func testCompilerCreatesGeometrySemanticWeightsAndInternalSource() throws {
    let source=body(),compiled=try AnatomyCompiler.compile(source)
    XCTAssertGreaterThan(compiled.mesh.indices.count,600)
    XCTAssertEqual(compiled.weights.count,compiled.mesh.vertices.count)
    XCTAssertEqual(compiled.anchors.count,compiled.mesh.vertices.count)
    XCTAssertEqual(compiled.structures.map(\.id),["bone"])
    XCTAssertEqual(compiled.joints,["arm","spine"])
    XCTAssertGreaterThan(compiled.report.regions["shoulder"] ?? 0,0)
    XCTAssertEqual(compiled.report.maximumDiscardedSkinWeight,0)
    for weight in compiled.weights {XCTAssertEqual(weight.weights.sum(),1,accuracy:0.00001)}
    let again=try AnatomyCompiler.compile(source)
    XCTAssertEqual(compiled.mesh.indices,again.mesh.indices)
    XCTAssertEqual(compiled.anchors,again.anchors)
  }
  func testCompatibleEditsPreserveMaterialCoordinatesAndAttachments() throws {
    let old=AnatomySource(id:"head",part:"head",elements:[AnatomyElement(id:"skull",region:"skull",center:.zero,radius:V3(0.4,0.6,0.3))],resolution:24)
    let anchor=try old.anchor(id:"groom-root",at:V3(0,0.6,0))
    var candidate=old
    candidate.elements[0].radius.y=0.8;candidate.elements[0].center=V3(0.2,0.1,0)
    candidate.elements[0].rotation.z=30;candidate.resolution=64
    let change=candidate.changes(from:old)
    XCTAssertTrue(change.rebuildRequired);XCTAssertFalse(change.rebindRequired)
    let result=try AnatomyCompiler.rebind([anchor],from:old,to:candidate)
    XCTAssertTrue(result.accepted)
    XCTAssertLessThan(length(result.positions[0]-candidate.elements[0].point(at:anchor.coordinate)),0.00001)
    XCTAssertEqual(result.anchors[0].id,"groom-root")
    XCTAssertLessThan(abs(try candidate.shape().value(at:result.positions[0])),0.0001)
  }
  func testStructuralRebindIsControlledAndAtomic() throws {
    let old=AnatomySource(id:"head",part:"head",elements:[AnatomyElement(id:"skull",region:"skull",center:.zero,radius:V3(repeating:0.5))])
    let anchors=[try old.anchor(id:"top",at:V3(0,0.5,0)),try old.anchor(id:"side",at:V3(0.5,0,0))]
    var candidate=old;candidate.elements[0].id="cranial-shell"
    let rejected=try AnatomyCompiler.rebind(anchors,from:old,to:candidate)
    XCTAssertFalse(rejected.accepted);XCTAssertEqual(rejected.anchors,anchors)
    let missingMap=try AnatomyCompiler.rebind(anchors,from:old,to:candidate,policy:AnatomyRebindPolicy(allowStructuralChanges:true))
    XCTAssertFalse(missingMap.accepted);XCTAssertEqual(missingMap.issues.count,2)
    let mapped=try AnatomyCompiler.rebind(anchors,from:old,to:candidate,
      policy:AnatomyRebindPolicy(allowStructuralChanges:true,elementMap:["skull":"cranial-shell"]))
    XCTAssertTrue(mapped.accepted);XCTAssertTrue(mapped.anchors.allSatisfy{$0.element=="cranial-shell"})
    candidate=old
    candidate.elements.append(AnatomyElement(id:"opening",region:"opening",operation:.cut,center:V3(0,0.5,0),radius:V3(repeating:0.2)))
    let cut=try AnatomyCompiler.rebind(anchors,from:old,to:candidate,policy:AnatomyRebindPolicy(allowStructuralChanges:true))
    XCTAssertFalse(cut.accepted);XCTAssertEqual(cut.anchors,anchors)
    XCTAssertTrue(cut.issues.contains{$0.anchor=="top"})
  }
  func testCapsuleCorrespondenceFollowsEndpointOrientation() throws {
    let element=AnatomyElement(id:"limb",region:"limb",primitive:.capsule,center:.zero,radius:V3(repeating:0.1),end:V3(0,1,0))
    try element.validate()
    let p=V3(0.1,0.5,0),coordinate=element.coordinate(at:p)
    XCTAssertLessThan(length(element.point(at:coordinate)-p),0.000001)
    var rotated=element;rotated.end=V3(2,0,0)
    let moved=rotated.point(at:coordinate)
    XCTAssertEqual(moved.x,1,accuracy:0.00001)
    XCTAssertEqual(rotated.shape.value(at:moved),0,accuracy:0.00001)
  }
  func testMalformedAnatomyRejectsBeforeCompilation() throws {
    var source=body();source.elements.append(source.elements[0])
    XCTAssertThrowsError(try AnatomyCompiler.compile(source))
    source=body();source.elements[0].radius.x = .nan
    XCTAssertThrowsError(try AnatomyCompiler.compile(source))
    source=body();source.elements[0].jointWeights=["spine":0.3]
    XCTAssertThrowsError(try source.validate())
    source=body();source.elements[0].operation = .cut
    XCTAssertThrowsError(try source.validate())
  }
  func testMergedOutputMembershipIsBackwardCompatibleAndDiagnosed() throws {
    let original=body()
    var merged=original;merged.replaces=["upper-limb","lower-limb"]
    try merged.validate()
    XCTAssertTrue(merged.changes(from:original).rebindRequired)
    XCTAssertTrue(merged.changes(from:original).reasons.contains{$0.contains("output membership")})
    let data=try JSONEncoder().encode(merged)
    XCTAssertEqual(try JSONDecoder().decode(AnatomySource.self,from:data),merged)
    var legacy=try XCTUnwrap(JSONSerialization.jsonObject(with:JSONEncoder().encode(original)) as? [String:Any])
    legacy.removeValue(forKey:"replaces")
    let decoded=try JSONDecoder().decode(AnatomySource.self,from:JSONSerialization.data(withJSONObject:legacy))
    XCTAssertEqual(decoded.replaces,[])
    merged.replaces=["upper-limb","upper-limb"];XCTAssertThrowsError(try merged.validate())
    merged.replaces=[merged.part];XCTAssertThrowsError(try merged.validate())
  }
  func testAnatomyAndPersistentAnchorsUseStrictCraftSchema() throws {
    let source=AnatomySource(id:"continuous-body",part:"body",elements:[
      AnatomyElement(id:"thorax",region:"thorax",center:.zero,radius:V3(repeating:0.5),jointWeights:["body":1])
    ],replaces:["upper-limb"])
    var craft=CreatureCraft();craft.anatomy=[source]
    craft.anatomyAnchors=[try source.anchor(id:"mane-root",at:V3(0,0.5,0))]
    let encoded=try JSONEncoder().encode(craft)
    XCTAssertEqual(try JSONDecoder().decode(CreatureCraft.self,from:encoded),craft)
    var raw=try XCTUnwrap(JSONSerialization.jsonObject(with:encoded) as? [String:Any])
    var anatomy=try XCTUnwrap(raw["anatomy"] as? [[String:Any]])
    var elements=try XCTUnwrap(anatomy[0]["elements"] as? [[String:Any]])
    elements[0]["raduis"]=[0.4,0.4,0.4];anatomy[0]["elements"]=elements;raw["anatomy"]=anatomy
    XCTAssertThrowsError(try JSONDecoder().decode(CreatureCraft.self,from:JSONSerialization.data(withJSONObject:raw)))
    raw=try XCTUnwrap(JSONSerialization.jsonObject(with:encoded) as? [String:Any])
    var anchors=try XCTUnwrap(raw["anatomyAnchors"] as? [[String:Any]])
    anchors[0]["triangleID"]=5;raw["anatomyAnchors"]=anchors
    XCTAssertThrowsError(try JSONDecoder().decode(CreatureCraft.self,from:JSONSerialization.data(withJSONObject:raw)))
  }
}
