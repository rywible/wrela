import XCTest
import FieldCore
import FieldCompiler
import FieldEngine
import simd

/// Exercises AssetSource's production compile/pose path without constructing a
/// renderer, opening an app or registering a species-specific generator.
final class SourceCraftIntegrationTests:XCTestCase {
  func testRuntimeReductionRetainsLocalizedAuthoringAndRejectsLostBindings() throws {
    try inProject {
      var source=AssetSource(id:"carved",name:"Carved surface",generator:"fields")
      let mesh=ParametricMesh.ellipsoid(.zero,V3(repeating:0.5),detail:64)
      var batch=SceneBatch(name:"shell",mesh:mesh,instances:[Instance()])
      batch.surfaceReduction=(0.2,0.001)
      source.craft.strokes=[SculptStroke(id:"small-relief",part:"shell",brush:.grab,
        points:[V3(0,0.5,0)],radius:0.13,strength:0.03)]
      let compiled=try source.finish([batch])[0].mesh
      XCTAssertLessThan(compiled.indices.count,mesh.indices.count/2)
      XCTAssertGreaterThan(compiled.vertices.map{$0.position.y}.max()!,0.528)
      XCTAssertEqual(compiled.vertices.map{$0.position.x}.max()!,0.5,accuracy:0.001)
      XCTAssertEqual(batch.mesh.vertices.map{$0.position.y}.max()!,0.5,accuracy:0.00001)
      batch.skinJoints=["shell"];batch.skinWeights=Array(repeating:SkinWeight(0),count:mesh.vertices.count)
      XCTAssertThrowsError(try source.finish([batch]),"A rigid policy cannot silently discard skin correspondence")
    }
  }
  private func inProject(_ body:() throws -> Void) rethrows {
    let previous=ProjectContext.current,projects=ProjectContext.projects
    let project=GameProject(id:"source-test",name:"Source test",defaultSubject:"source",directory:URL(fileURLWithPath:"/tmp/wrela-source-test"),generators:[],create:{_ in throw RuntimeError.message("CPU fixture has no game host")})
    ProjectContext.configure([project])
    defer {ProjectContext.projects=projects;ProjectContext.current=previous}
    try body()
  }

  func testSourceOnlyRigClipAndContactUseProductionEvaluator() throws {
    try inProject {
      var source=AssetSource(id:"source",name:"Source only",generator:"fields")
      source.craft.anatomy=[AnatomySource(id:"shell",part:"body",elements:[AnatomyElement(id:"shell",region:"thorax",center:V3(0,1,0),radius:V3(0.3,0.3,0.3))],resolution:24)]
      source.craft.joints=[PartJoint(id:"body"),PartJoint(id:"upper",parent:"body",pivot:V3(0,1,0)),
        PartJoint(id:"lower",parent:"upper",pivot:V3(0,0.55,0.15)),PartJoint(id:"foot",parent:"lower",pivot:V3(0,0.12,0))]
      source.craft.contactChains=[ContactChain(id:"support",parent:"body",upper:"upper",lower:"lower",foot:"foot",pole:V3(0,0,1),sole:0.12)]
      source.craft.phrases=[MotionPhrase(id:"compress",duration:2,samples:[PoseSample(time:0,poses:["body":JointPose()]),
        PoseSample(time:1,poses:["body":JointPose(offset:V3(0,-0.15,0))]),PoseSample(time:2,poses:["body":JointPose()])],
        contacts:[ContactPath(chain:"support",keys:[ContactKey(time:0,position:V3(0,0.12,0),planted:true),ContactKey(time:2,position:V3(0,0.12,0),planted:true)])])]
      try source.validate()
      XCTAssertNil(source.definition);XCTAssertNil(source.motion)
      XCTAssertTrue(source.animation?.clips.contains("compress") == true)
      let result=source.evaluatedPose(clip:"compress",time:1,state:BehaviorSnapshot())
      XCTAssertEqual(result.contacts.count,1);XCTAssertLessThan(result.contacts[0].residual,0.001)
      XCTAssertEqual(result.matrices["body"]!.columns.3.y,-0.15,accuracy:0.00001)
      let batches=try source.compile();XCTAssertEqual(batches.map(\.name),["body"])
      XCTAssertGreaterThan(batches[0].mesh.vertices.count,100)
      let decoded=try AssetSource.decode(JSONEncoder().encode(source))
      XCTAssertTrue(decoded.animation?.clips.contains("compress") == true)
      XCTAssertEqual(decoded.craft,source.craft)
    }
  }

  func testMergedOutputKeepsJointAndFiltersCachedBaseGeometry() throws {
    try inProject {
      var source=AssetSource(id:"merge",name:"Merged source",generator:"fields",parts:[AssetPart(name:"old-limb",field:FieldExpression(op:"sphere",values:[0.1]),color:[0.5,0.5,0.5])])
      source.craft.anatomy=[AnatomySource(id:"continuous",part:"body",elements:[AnatomyElement(id:"body",region:"body",center:.zero,radius:V3(repeating:0.3))],resolution:24,replaces:["old-limb"])]
      let cachedBase=try source.compileBase()
      XCTAssertEqual(Set(cachedBase.map(\.name)),["old-limb","body"])
      XCTAssertTrue(source.joints.contains{$0.id=="old-limb"})
      XCTAssertEqual(try source.finish(cachedBase).map(\.name),["body"])
      source.craft.anatomy[0].replaces=[]
      XCTAssertEqual(Set(try source.finish(cachedBase).map(\.name)),["old-limb","body"],"Changing output membership must work with the same cached base")
    }
  }

  func testBoundSourceMovesCompiledGroomAndResolvedFinish() throws {
    try inProject {
      var source=AssetSource(id:"binding",name:"Bound source",generator:"fields",parts:[AssetPart(name:"hair",field:FieldExpression(op:"sphere",values:[0.001]),color:[1,1,1])])
      let original=AnatomySource(id:"skin",part:"body",elements:[AnatomyElement(id:"skin",region:"shoulder",center:V3(0,1,0),radius:V3(repeating:1))],resolution:24)
      let anchor=try original.anchor(id:"root",at:V3(0,2,0))
      source.craft.anatomy=[original];source.craft.anatomyAnchors=[anchor]
      source.craft.guideChains=[CraftGuideChain(id:"lock",joint:"body",points:[V3(0,2,0),V3(0,2.2,0.1),V3(0,2.4,0.2)])]
      source.craft.grooms=[GroomDesign(id:"hair",part:"hair",guides:[["lock-0","lock-1","lock-2"]],fibres:4)]
      source.surfaceLayers=[SurfaceLayer(id:"finish",part:"hair",center:V3(0,2,0),radius:V3(repeating:0.2),tint:V3(repeating:0.5))]
      source.craft.anatomyBindings=[AnatomyBinding(id:"groom",anchor:"root",target:"lock",kind:.guideChain,bindPosition:anchor.sourcePosition,bindNormal:anchor.sourceNormal),
        AnatomyBinding(id:"detail",anchor:"root",target:"finish",kind:.surfaceLayer,bindPosition:anchor.sourcePosition,bindNormal:anchor.sourceNormal)]
      let before=try source.compile().first{$0.name=="hair"}!.mesh
      var updated=original;updated.elements[0].radius.y=1.2
      source.craft.anatomy=[updated]
      source.craft.anatomyAnchors=try AnatomyCompiler.rebind([anchor],from:original,to:updated).anchors
      let after=try source.compile().first{$0.name=="hair"}!.mesh
      XCTAssertEqual(before.vertices.count,after.vertices.count)
      for i in before.vertices.indices {XCTAssertEqual(after.vertices[i].position.y-before.vertices[i].position.y,0.2,accuracy:0.00001)}
      XCTAssertEqual(source.resolvedSurfaceLayers[0].center.y,2.2,accuracy:0.00001)
      XCTAssertEqual(source.resolvedSecondaryRig.nodes.first{$0.id=="lock-0"}!.position.y,2.2,accuracy:0.00001)
    }
  }
}
