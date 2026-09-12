import XCTest
import FieldCore
import FieldCompiler
import simd

final class CreatureCraftTests:XCTestCase {
  func testAuthoredGuidesAndClothUseSharedCompiler() throws {
    let chain=CraftGuideChain(id:"crest",joint:"head",points:[V3(0,1,0),V3(0,1.2,0.1),V3(0.1,1.3,0.2)])
    try chain.validate(joints:["head"])
    XCTAssertEqual(chain.rig.nodes[0].inverseMass,0);XCTAssertTrue(chain.rig.links[0].contact)
    let groom=GroomDesign(id:"crest-hair",part:"hair",guides:[["crest-0","crest-1","crest-2"]],fibres:8)
    let (a,skin,ids)=try CraftGroom.compile(groom,rig:chain.rig)
    let (b,_,_)=try CraftGroom.compile(groom,rig:chain.rig)
    XCTAssertEqual(a.vertices.map(\.position),b.vertices.map(\.position))
    XCTAssertTrue(skin.allSatisfy{$0.validate(count:ids.count)})
    let cloth=ClothPanel(id:"cape",part:"cloth",columns:2,rows:2,
      points:[V3(-0.5,1,0),V3(0.5,1,0),V3(-0.5,1,1),V3(0.5,1,1)],attachments:Array(repeating:"head",count:4),pins:[0,1],resolution:8)
    try cloth.validate(joints:["head"])
    let (mesh,weights,joints)=try ClothCompiler.compile(cloth,rig:cloth.rig)
    XCTAssertEqual(mesh.vertices.count,81);XCTAssertEqual(weights.count,81);XCTAssertEqual(joints.count,4)
    XCTAssertTrue(cloth.rig.nodes.allSatisfy{$0.frame == .surface})
    XCTAssertTrue(weights.allSatisfy{$0.validate(count:4)})
    for v in mesh.vertices {XCTAssertEqual(v.position.y,1,accuracy:0.00001)}
    var invalid=cloth;invalid.columns=Int.max
    XCTAssertTrue(invalid.rig.nodes.isEmpty);XCTAssertThrowsError(try invalid.validate(joints:["head"]))
    var craft=CreatureCraft();craft.guideChains=[chain];craft.clothPanels=[cloth]
    let decoded=try JSONDecoder().decode(CreatureCraft.self,from:JSONEncoder().encode(craft))
    XCTAssertEqual(decoded,craft)
  }
  func testPoseFittingRespectsBoundsAndUnselectedChannels() throws {
    let joints=[PartJoint(id:"root"),PartJoint(id:"elbow",parent:"root",pivot:V3(0,1,0))]
    let poses=["root":JointPose(offset:V3(0,0,0.17)),"elbow":JointPose(rotation:V3(0,0,-8))]
    let freedoms=[PoseFreedom(joint:"root",channel:"roll",minimum:-70,maximum:70),PoseFreedom(joint:"elbow",channel:"roll",minimum:-100,maximum:100)]
    let target=PoseTarget(joint:"elbow",point:V3(0,2,0),target:V3(0.4,1.7,0.17))
    let result=try PoseFitting.solve(joints:joints,poses:poses,targets:[target],freedoms:freedoms,iterations:60)
    XCTAssertTrue(result.converged);XCTAssertLessThan(result.maximumError,0.002)
    XCTAssertEqual(result.poses["root"]!.offset,poses["root"]!.offset)
    let impossible=try PoseFitting.solve(joints:joints,poses:poses,targets:[PoseTarget(joint:"elbow",point:V3(0,2,0),target:V3(5,5,0))],freedoms:freedoms)
    XCTAssertFalse(impossible.converged);XCTAssertGreaterThan(impossible.maximumError,1)
    XCTAssertLessThanOrEqual(abs(impossible.poses["root"]!.rotation.z),70)
  }
  func testSplinePassesThroughUnevenKeysWithContinuousVelocity() throws {
    let p=MotionPhrase(id:"flow",duration:3,samples:[PoseSample(time:0,poses:["body":JointPose()]),
      PoseSample(time:1,poses:["body":JointPose(offset:V3(1,0,0),rotation:V3(0,20,0))]),
      PoseSample(time:3,poses:["body":JointPose(offset:V3(3,0,0),rotation:V3(0,70,0))])],interpolation:.spline)
    try p.validate(joints:["body"],chains:[])
    let e:Float=0.001,a=p.pose(at:1-e)["body"]!,b=p.pose(at:1)["body"]!,c=p.pose(at:1+e)["body"]!
    XCTAssertEqual(b.offset.x,1,accuracy:0.00001)
    XCTAssertEqual((b.offset.x-a.offset.x)/e,(c.offset.x-b.offset.x)/e,accuracy:0.01)
    XCTAssertEqual((b.rotation.y-a.rotation.y)/e,(c.rotation.y-b.rotation.y)/e,accuracy:0.1)
    XCTAssertGreaterThan((c.offset.x-a.offset.x)/(2*e),0.95)
  }
  func testSupportTransferAnticipatesLiftoffAndEasesLanding() throws {
    let paths=try FootstepPlan.compile([AuthoredFootstep(chain:"a",lift:1,land:2,target:V3(1,0,0))],initial:["a":.zero],duration:3,minimumSupport:0)
    let p=paths[0]
    XCTAssertEqual(p.supportWeight(0.5,transition:0.25),1)
    XCTAssertEqual(p.supportWeight(0.875,transition:0.25),0.5,accuracy:0.00001)
    XCTAssertLessThan(p.supportWeight(0.999,transition:0.25),0.00001)
    XCTAssertEqual(p.supportWeight(1,transition:0.25),0)
    XCTAssertEqual(p.supportWeight(2,transition:0.25),0)
    XCTAssertEqual(p.supportWeight(2.125,transition:0.25),0.5,accuracy:0.00001)
    let speed=(p.sample(1.501).position.x-p.sample(1.499).position.x)/0.002
    XCTAssertGreaterThan(speed,0.95)
  }
  func testOverlappingSupportTransfersHaveNoBoundaryPop() throws {
    let points=["a":V3(-1,0,-1),"b":V3(1,0,-1),"c":V3(-1,0,1),"d":V3(1,0,1)]
    let paths=try FootstepPlan.compile([
      AuthoredFootstep(chain:"a",lift:1,land:2,target:points["a"]!),
      AuthoredFootstep(chain:"d",lift:2.1,land:3,target:points["d"]!)],initial:points,duration:4,minimumSupport:3)
    let joints=[PartJoint(id:"body")],mass=[BodyMass(joint:"body",center:V3(-0.5,1,-0.3),kilograms:10)]
    func position(_ time:Float)->V3 {
      let targets=Dictionary(uniqueKeysWithValues:paths.map{($0.chain,$0.sample(time))})
      let weights=Dictionary(uniqueKeysWithValues:paths.map{($0.chain,$0.supportWeight(time,transition:0.4))})
      return BodySupport.assist(BalanceControl(joint:"body",maximumShift:2),joints:joints,poses:[:],masses:mass,targets:targets,supportWeights:weights)["body"]?.offset ?? .zero
    }
    for boundary:Float in [1,2,2.1,3] {
      XCTAssertLessThan(length(position(boundary+0.0001)-position(boundary-0.0001)),0.001,"Support must transfer continuously at \(boundary)")
    }
    var previous=position(0)
    for tick in 1...240 {
      let time=Float(tick)/60,current=position(time)
      XCTAssertLessThan(length(current-previous),0.055,"Abrupt frame at \(time): \(previous) -> \(current)")
      previous=current
    }
  }
  func testFootstepPlanningRejectsAccidentalSupportLoss() throws {
    let initial=["a":V3(-1,0,0),"b":V3(1,0,0),"c":V3(0,0,1)]
    let steps=[AuthoredFootstep(chain:"a",lift:1,land:2,target:V3(-1,0,-0.4),height:0.3)]
    let paths=try FootstepPlan.compile(steps,initial:initial,duration:3,minimumSupport:2)
    let a=paths.first{$0.chain=="a"}!
    XCTAssertTrue(a.sample(0.9).planted);XCTAssertFalse(a.sample(1).planted)
    XCTAssertEqual(a.sample(1.5).position.y,0.3,accuracy:0.00001)
    XCTAssertTrue(a.sample(2).planted);XCTAssertEqual(a.sample(2.5).position,V3(-1,0,-0.4))
    XCTAssertThrowsError(try FootstepPlan.compile(steps+[AuthoredFootstep(chain:"b",lift:1.5,land:2.5,target:V3(1,0,-0.4))],initial:initial,duration:3,minimumSupport:2))
    XCTAssertThrowsError(try FootstepPlan.compile(steps+steps,initial:initial,duration:3,minimumSupport:0))
  }
  func testSpatialStrokeSymmetryAndSamplingInvariance() throws {
    let stroke=SculptStroke(id:"cheek",part:"mask",brush:.grab,points:[V3(0.6,0,0),V3(0.6,1,0)],radius:0.3,strength:0.04,direction:V3(1,0,0),mirrorX:true)
    try stroke.validate()
    let a=stroke.field(V3(0.6,0.4,0)),b=stroke.field(V3(-0.6,0.4,0))
    XCTAssertEqual(a.position.x,-b.position.x,accuracy:0.000001)
    var split=stroke;split.points.insert(V3(0.6,0.5,0),at:1)
    XCTAssertLessThan(length(split.field(V3(0.65,0.2,0)).position-stroke.field(V3(0.65,0.2,0)).position),0.000001)
    XCTAssertEqual(stroke.field(V3(3,0,0)).position,V3(3,0,0))
  }
  func testRefinementPropagatesSharedEdgesAndSkin() throws {
    let mesh=ParametricMesh.surface(u:1,v:1,position:{u,v in V3(u,v,0)},tint:{_,_ in V3(repeating:1)})
    let skin=mesh.vertices.map{SkinWeight(0,1,blend:$0.position.x)}
    let (refined,weights)=try SculptCompiler.refine(mesh,weights:skin,levels:1)
    XCTAssertEqual(refined.indices.count,mesh.indices.count*4)
    XCTAssertEqual(refined.vertices.count,9);XCTAssertEqual(weights.count,refined.vertices.count)
    for i in weights.indices {
      XCTAssertTrue(weights[i].validate(count:2))
      var right:Float=0;for k in 0..<4 where weights[i].joints[k]==1 {right+=weights[i].weights[k]}
      XCTAssertEqual(right,refined.vertices[i].position.x,accuracy:0.000001)
    }
    XCTAssertThrowsError(try SculptCompiler.refine(mesh,weights:skin,levels:3))
  }
  func testFlattenAndMissedFoldRejection() throws {
    let mesh=ParametricMesh.ellipsoid(.zero,V3(repeating:0.5),detail:12)
    var s=SculptStroke(id:"plane",part:"mask",brush:.flatten,points:[V3(0,0.4,0)],radius:0.5,strength:0.2)
    let changed=try SculptCompiler.apply([s],to:mesh)
    XCTAssertNotEqual(changed.vertices.map(\.position),mesh.vertices.map(\.position))
    s.points=[V3(10,0,0)];XCTAssertThrowsError(try SculptCompiler.apply([s],to:mesh))
    s.points=[V3(0,0.5,0)];s.brush = .grab;s.strength = -1;s.radius=0.1
    XCTAssertThrowsError(try SculptCompiler.apply([s],to:mesh))
  }
  func testSkinFieldAddsJointWithoutChangingOutsideRegion() throws {
    let mesh=ParametricMesh.surface(u:4,v:4,position:{u,v in V3(u,v,0)},tint:{_,_ in V3(repeating:1)})
    let f=SkinField(id:"shoulder",part:"body",center:.zero,radius:V3(repeating:0.8),jointWeights:["shoulder":1])
    try f.validate(joints:["body","shoulder"])
    let (ids,w)=try SkinFieldCompiler.apply([f],mesh:mesh,joints:[],weights:[],fallback:"body")
    XCTAssertEqual(ids,["body","shoulder"])
    XCTAssertTrue(w.allSatisfy{$0.validate(count:2)})
    XCTAssertEqual(w.first!.joints[0],1);XCTAssertEqual(w.first!.weights[0],1)
    XCTAssertEqual(w.last!.joints[0],0);XCTAssertEqual(w.last!.weights[0],1)
    var bad=f;bad.jointWeights=["missing":1];XCTAssertThrowsError(try bad.validate(joints:["body"]))
  }
  func phrase()->MotionPhrase {
    MotionPhrase(id:"turn",duration:2,samples:[PoseSample(time:0,poses:["body":JointPose()]),PoseSample(time:2,poses:["body":JointPose(offset:V3(1,0,0),rotation:V3(0,90,0))])],contacts:[ContactPath(chain:"left",keys:[ContactKey(time:0,position:V3(-1,0,0),planted:true),ContactKey(time:2,position:V3(-1,0,0),planted:true)])])
  }
  func testPhraseRetimingMirroringAndQuaternionInterpolation() throws {
    var p=phrase()
    p.references=[MotionReference(id:"reference",locator:"",sourceTime:0,phraseTime:0,note:"Measured body point",landmarks:["body":V3(1,2,3)])]
    try p.validate(joints:["body"],chains:["left"])
    let q=p.retimed(to:4);try q.validate(joints:["body"],chains:["left"])
    XCTAssertEqual(p.pose(at:1)["body"]!.rotation.y,q.pose(at:2)["body"]!.rotation.y,accuracy:0.0001)
    XCTAssertEqual(q.pose(at:2)["body"]!.rotation.y,45,accuracy:0.0001)
    let mirror=try p.mirrored(id:"right-turn",jointMap:["body":"root"],chainMap:["left":"right"])
    try mirror.validate(joints:["root"],chains:["right"])
    XCTAssertEqual(mirror.samples.last!.poses["root"]!.offset.x,-1)
    XCTAssertEqual(mirror.contacts[0].keys[0].position.x,1)
    XCTAssertEqual(mirror.references[0].landmarks,["root":V3(-1,2,3)])
    let twice=try mirror.mirrored(id:"turn",jointMap:["root":"body"],chainMap:["right":"left"])
    XCTAssertEqual(twice,p)
    let a=JointPose(rotation:V3(0,170,0)),b=JointPose(rotation:V3(0,-170,0))
    let halfway=PoseBlending.quaternion(PoseBlending.blend(a,b,0.5).rotation).act(V3(0,0,-1))
    XCTAssertGreaterThan(halfway.z,0.999)
  }
  func testPlantedMovingPathsAndMismatchedPoseSetsRejected() throws {
    var p=phrase();p.contacts[0].keys[1].position.x+=0.1
    XCTAssertThrowsError(try p.validate(joints:["body"],chains:["left"]))
    p=phrase();p.samples[1].poses["extra"]=JointPose()
    XCTAssertThrowsError(try p.validate(joints:["body","extra"],chains:["left"]))
  }
  func testArrangementPreservesContactsAndLabelsMovingBlends() throws {
    let p=phrase(),a=MotionArrangement(clip:"walk",duration:2,looping:false,layers:[PhraseLayer(id:"turn",phrase:"turn",start:0,end:2,sourceStart:0,sourceEnd:2,fadeIn:1)])
    try a.validate(phrases:[p],clips:["walk"])
    let original:[String:ContactTarget]=["left":ContactTarget(V3(-2,0,0),planted:true)]
    let middle=a.evaluate(time:0.5,phrases:[p],poses:[:],contacts:original)
    XCTAssertFalse(middle.contacts["left"]!.planted)
    let end=a.evaluate(time:2,phrases:[p],poses:[:],contacts:original)
    XCTAssertTrue(end.contacts["left"]!.planted);XCTAssertEqual(end.contacts["left"]!.position.x,-1)
    XCTAssertEqual(end.poses["body"]!.offset.x,1)
  }
  func testMassSupportAndBoundedAssistance() throws {
    let rig=[PartJoint(id:"body")],masses=[BodyMass(joint:"body",center:V3(2,1,0),kilograms:100)]
    let anchors=[V3(-1,0,-1),V3(1,0,-1),V3(1,0,1),V3(-1,0,1)]
    let before=BodySupport.inspect(masses:masses,matrices:PartRig.matrices(rig),anchors:anchors)
    XCTAssertEqual(before.outsideDistance,1,accuracy:0.00001);XCTAssertFalse(before.supported)
    let targets=Dictionary(uniqueKeysWithValues:anchors.enumerated().map{("\($0.offset)",ContactTarget($0.element,planted:true))})
    let result=BodySupport.assist(BalanceControl(joint:"body",maximumShift:0.3),joints:rig,poses:[:],masses:masses,targets:targets)
    XCTAssertEqual(result["body"]!.offset.x,-0.3,accuracy:0.00001)
    XCTAssertEqual(result["body"]!.offset.y,0)
    XCTAssertFalse(BodySupport.inspect(masses:masses,matrices:[:],anchors:[]).supported)
    XCTAssertEqual(BodySupport.hull([SIMD2(0,0),SIMD2(1,0),SIMD2(0.5,0),SIMD2(0,0)]).count,2)
  }
  func testCorrectiveAndTriangleSkinDiagnostics() throws {
    let mesh=ParametricMesh.surface(u:2,v:2,position:{u,v in V3(u,v,0)},tint:{_,_ in V3(repeating:1)})
    let f=SurfaceEdit(id:"bend",part:"body",center:V3(0.5,0.5,0),radius:V3(repeating:2),offset:V3(0,0,0.1))
    let c=PoseCorrective(id:"bend",driver:"body",axis:0,start:0,end:90,field:f)
    try c.validate(joints:["body"]);XCTAssertEqual(c.activation(JointPose(rotation:V3(45,0,0))),0.5,accuracy:0.00001)
    XCTAssertEqual(MemoryLayout<CorrectiveUniform>.stride,64)
    let corrected=DeformationAudit.corrected(mesh,fields:[f])
    let authored=try SurfaceEditing.apply([f],to:mesh)
    for i in mesh.vertices.indices {XCTAssertEqual(corrected.vertices[i].position,authored.vertices[i].position)}
    var scale=matrix_identity_float4x4;scale[0].x=2
    let posed=DeformationAudit.deformed(mesh,weights:[],palette:[],rigid:scale)
    let metrics=DeformationAudit.inspect(rest:mesh,posed:posed)
    XCTAssertEqual(metrics.maximumStretch,2,accuracy:0.000001);XCTAssertEqual(metrics.reversedTriangles,0)
  }
  func testSurfaceAttachmentsHaveBarycentricSkin() {
    let mesh=ParametricMesh.surface(u:1,v:1,position:{u,v in V3(u,v,0)},tint:{_,_ in V3(repeating:1)})
    let weights=mesh.vertices.map{SkinWeight(0,1,blend:$0.position.x)}
    let a=CostumeCompiler.attach(V3(0.3,0.7,0.1),to:mesh,weights:weights)
    XCTAssertLessThan(length(a.position-V3(0.3,0.7,0)),0.00001)
    var right:Float=0;for k in 0..<4 where a.skin.joints[k]==1 {right+=a.skin.weights[k]}
    XCTAssertEqual(right,0.3,accuracy:0.00001)
    XCTAssertEqual(a.distance,0.1,accuracy:0.00001)
  }
  func testSchemaRejectsNestedTyposAndBooleansAsNumbers() throws {
    var c=CreatureCraft();c.landmarks=[AnatomyLandmark(id:"eye",part:"mask",position:.zero)]
    let data=try JSONEncoder().encode(c);let raw=try JSONSerialization.jsonObject(with:data)
    try CraftSchema.validate(raw)
    XCTAssertThrowsError(try CraftSchema.validate(["strokes":[["id":"a","strenght":1]]]))
    XCTAssertThrowsError(try CraftSchema.validate(["refinement":["mask":true]]))
    XCTAssertThrowsError(try CraftSchema.validate(["version":1.5]))
    XCTAssertEqual(try JSONDecoder().decode(CreatureCraft.self,from:data),c)
    XCTAssertEqual(try JSONDecoder().decode(CreatureCraft.self,from:Data("{}".utf8)),CreatureCraft())
  }
  func testRigExtensionsRejectCyclesAndMissingParents() throws {
    let base=[PartJoint(id:"body"),PartJoint(id:"neck",parent:"body")]
    var craft=CreatureCraft();craft.joints=[PartJoint(id:"spine",parent:"body"),PartJoint(id:"neck",parent:"spine")]
    try PartRig.validate(craft.resolvedJoints(base))
    craft.joints[0].parent="neck";XCTAssertThrowsError(try PartRig.validate(craft.resolvedJoints(base)))
  }
}
