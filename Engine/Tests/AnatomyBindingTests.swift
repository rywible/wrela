import XCTest
import FieldCore
import FieldCompiler
import simd

final class AnatomyBindingTests:XCTestCase {
  func testCompatibleSurfaceEditMovesGroomDetailAndRigidAttachment() throws {
    let original=AnatomySource(id:"torso-shape",part:"torso",elements:[AnatomyElement(id:"skin",region:"shoulder",center:.zero,radius:V3(repeating:1))],resolution:24)
    let anchor=try original.anchor(id:"shoulder-root",at:V3(0,1,0))
    var craft=CreatureCraft();craft.anatomy=[original];craft.anatomyAnchors=[anchor]
    craft.joints=[PartJoint(id:"torso"),PartJoint(id:"ornament",parent:"torso")]
    craft.guideChains=[CraftGuideChain(id:"lock",joint:"torso",points:[V3(0,1,0),V3(0,1.2,0.1),V3(0,1.4,0.2)])]
    craft.skinFields=[SkinField(id:"paint",part:"torso",center:V3(0,1,0),radius:V3(repeating:0.2),jointWeights:["torso":1])]
    craft.correctives=[PoseCorrective(id:"fold",driver:"torso",axis:0,start:0,end:30,field:SurfaceEdit(id:"fold",part:"torso",center:V3(0,1,0),radius:V3(repeating:0.2),offset:V3(0,0.01,0)))]
    for (kind,target) in [(AnatomyBindingKind.guideChain,"lock"),(.joint,"ornament"),(.skinField,"paint"),(.corrective,"fold"),(.surfaceLayer,"speckle")] {
      craft.anatomyBindings.append(AnatomyBinding(id:target,anchor:anchor.id,target:target,kind:kind,bindPosition:anchor.sourcePosition,bindNormal:anchor.sourceNormal))
    }
    let groom=GroomDesign(id:"hair",part:"hair",guides:[["lock-0","lock-1","lock-2"]],fibres:4)
    let before=try CraftGroom.compile(groom,rig:craft.boundGuideChains[0].rig).0
    var candidate=original;candidate.elements[0].radius.y=1.2
    let rebound=try AnatomyCompiler.rebind([anchor],from:original,to:candidate)
    XCTAssertTrue(rebound.accepted);craft.anatomy=[candidate];craft.anatomyAnchors=rebound.anchors
    let after=try CraftGroom.compile(groom,rig:craft.boundGuideChains[0].rig).0
    XCTAssertEqual(before.vertices.count,after.vertices.count)
    for i in before.vertices.indices {
      XCTAssertEqual(after.vertices[i].position.y-before.vertices[i].position.y,0.2,accuracy:0.00001)
      XCTAssertEqual(after.vertices[i].position.x,before.vertices[i].position.x,accuracy:0.00001)
    }
    XCTAssertEqual(craft.boundSkinFields[0].center.y,1.2,accuracy:0.00001)
    XCTAssertEqual(craft.boundCorrectives[0].field.center.y,1.2,accuracy:0.00001)
    let layer=SurfaceLayer(id:"speckle",part:"torso",center:V3(0,1,0),radius:V3(repeating:0.2),tint:V3(repeating:0.5))
    XCTAssertEqual(craft.boundSurfaceLayers([layer])[0].center.y,1.2,accuracy:0.00001)
    XCTAssertEqual(craft.resolvedJoints([]).first{$0.id=="ornament"}!.offset.y,0.2,accuracy:0.00001)
    XCTAssertEqual(craft.guideChains[0].points[0].y,1,"Resolving a binding cannot mutate authored controls or accumulate drift")
    let encoded=try JSONEncoder().encode(craft)
    XCTAssertEqual(try JSONDecoder().decode(CreatureCraft.self,from:encoded),craft)
  }

  func testBoundGuideNormalFrameAndDuplicateTargetRejection() throws {
    let source=AnatomySource(id:"skin",part:"body",elements:[AnatomyElement(id:"skin",region:"skin",center:.zero,radius:V3(repeating:1))])
    var anchor=try source.anchor(id:"root",at:V3(0,1,0))
    let binding=AnatomyBinding(id:"curve",anchor:anchor.id,target:"lock",kind:.guideChain,bindPosition:anchor.sourcePosition,bindNormal:anchor.sourceNormal,followNormal:true)
    anchor.sourcePosition=V3(1,0,0);anchor.sourceNormal=V3(1,0,0)
    let p=binding.position(V3(0,1.25,0),anchors:[anchor])
    XCTAssertEqual(p.x,1.25,accuracy:0.00001);XCTAssertEqual(p.y,0,accuracy:0.00001)
    var craft=CreatureCraft();craft.anatomy=[source];craft.anatomyAnchors=[try source.anchor(id:"root",at:V3(0,1,0))]
    craft.joints=[PartJoint(id:"body")];craft.guideChains=[CraftGuideChain(id:"lock",joint:"body",points:[V3(0,1,0),V3(0,2,0)])]
    craft.anatomyBindings=[binding];var duplicate=binding;duplicate.id="duplicate";craft.anatomyBindings.append(duplicate)
    XCTAssertThrowsError(try craft.validate(parts:["body"],rig:craft.joints,guides:["lock-0","lock-1"],chains:[],clips:[]))
  }
}
