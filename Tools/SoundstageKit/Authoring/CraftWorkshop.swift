import FieldCore
import FieldCompiler
import FieldEngine
import Foundation
import simd

extension WorkshopRenderer {
  func craftSnapshot() throws -> [String:Any] {
    ["revision":sourceRevision,"source":try craftJSON(studioSource.craft),"schema":CraftSchema.json,
      "parts":studioBatches.map{["id":$0.name,"vertices":$0.vertices.length/MemoryLayout<Vertex>.stride,"skinJoints":$0.skinJoints] as [String:Any]},
      "joints":try craftJSON(studioSource.joints),"guideNodes":try craftJSON(studioSource.resolvedSecondaryRig.nodes),
      "resolvedLandmarks":try craftJSON(studioSource.craft.boundLandmarks),
      "resolvedSurfaceLayers":try craftJSON(studioSource.resolvedSurfaceLayers),
      "anatomy":studioSource.craft.anatomy.map{source in ["id":source.id,"part":source.part,"replaces":source.replaces,"regions":Array(Set(source.elements.map(\.region))).sorted(),"anchors":studioSource.craft.anatomyAnchors.filter{$0.source==source.id}.count,"internalStructures":source.internalElements.map(\.id)] as [String:Any]},
      "clips":studioSource.animation?.clips ?? [],"contactChains":studioSource.animation?.contactChains.map(\.id) ?? [],
      "operations":[
        "craft {expectedRevision,operations:[{op:upsert,collection:anatomy|anatomyAnchors|anatomyBindings|contactChains|strokes|sculptLayers|detailPatches|landmarks|joints|skinFields|correctives|phrases|masses|guideChains|clothPanels|grooms|seams,value:record}]}",
        "capture returns metadata.sculptFrame; sculptProbe: frame token, expectedRevision, pixels[[x,y]], optional part/explain; actual rendered triangle to bind/source selection and local parameter response",
        "fitSculpt: key=new layer, part, controls[{center,radius}], targets[{id,point,target,tolerance}], protect optional, maximumDisplacement, maximumNormalChangeDegrees, iterations; bounded source-field fit with atomic rejection",
        "sculptOverlay: expectedRevision, value=null or {part,controls,protect}; orange influence and blue protection on native geometry; study-only",
        "frameVisible: expectedRevision, optional part; frame currently visible deformed surfaces, excluding hidden parts and motion envelopes",
        "sculptFrameLock: expectedRevision, enabled; freeze/release the framing and scene-placement basis for matched sculpt alternatives",
        "remove: collection and key; set: collection=refinement|arrangement|balance, value=object (null clears optional values)",
        "capturePhrase: key, duration, samples; captures current clip into a reusable full-pose/contact phrase",
        "retimePhrase: key, duration; mirrorPhrase: key, newKey, jointMap, chainMap; retargetPhrase: key, newKey, jointMap, chainMap, scale",
        "anatomyAnchor: key, source, point, maximumProjection optional; bounded projection records a persistent semantic surface coordinate",
        "bindAnatomy: key, anchor, kind=guideChain|guideNode|joint|landmark|skinField|corrective|surfaceLayer, target, followNormal; capture a persistent consumer binding",
        "rebindAnatomy: key=source, allowStructuralChanges, maximumProjection, tolerance, elementMap; explicit bounded recovery in the same transaction as a replacement",
        "editInterval: key=phrase, edit={start,end,fadeIn,fadeOut,adjustments:[{joint,offset,rotation,delay}],bounds,protectedTimes}; preserve contacts and protected timing",
        "rigChain: key prefix, parent, points[2...16], reparent[]; adds a serial chain and reparents explicit children",
        "fitPose: key, time, targets[{joint,point,target,weight}], freedoms[{joint,channel,minimum,maximum}]; bounded landmark fitting with explicit residual rejection",
        "posedCorrective: key, part, point, offset, radius, driver, axis, start, end, time optional; fit a visible body surface displacement into a source corrective at the current clip",
        "footsteps: key, steps[{chain,lift,land,target,height}], minimumSupport; compile footfalls with a support-count gate",
        "craftReport: samples=2...121, part optional, start/end seconds optional; motion/support always, surface deformation only for named part"],
      "units":"Bind coordinates in metres; right-handed XYZ joint rotations in degrees, forward -Z, up Y. Mass in kg.",
      "limits":"Schema plus semantic bounds apply atomically. Capture/retarget assists authoring; it does not infer acting or certify physical motion."]
  }
  func craftJSON<T:Encodable>(_ value:T) throws -> Any {try JSONSerialization.jsonObject(with:JSONEncoder().encode(value))}

  @discardableResult func editCraft(_ request:[String:Any]) throws -> [String:Any] {
    try CraftError.require(Set(request.keys).isSubset(of:["id","action","expectedRevision","operations"])
      && request["expectedRevision"] as? String==sourceRevision,"transaction.revision","Supply the current revision from craftInspect")
    guard let operations=request["operations"] as? [[String:Any]],(1...128).contains(operations.count) else {throw CraftError(path:"transaction.operations",reason:"Use 1…128 operations")}
    var candidate=studioSource
    var document=try craftJSON(candidate.craft) as! [String:Any]
    var rebindPolicies:[String:AnatomyRebindPolicy]=[:]
    var posedFits:[[String:Any]]=[]
    var sculptFits:[[String:Any]]=[]
    let arrays=Set(["anatomy","anatomyAnchors","anatomyBindings","contactChains","landmarks","strokes","joints","skinFields","correctives","phrases","masses","guideChains","clothPanels","grooms","seams","sculptLayers","detailPatches"])
    func decodeDocument() throws -> CreatureCraft {
      try CraftSchema.validate(document)
      return try JSONDecoder().decode(CreatureCraft.self,from:JSONSerialization.data(withJSONObject:document))
    }
    func number(_ operation:[String:Any],_ key:String,_ fallback:Float?=nil) throws -> Float {
      if operation[key]==nil,let fallback {return fallback}
      guard let n=operation[key] as? NSNumber,CFGetTypeID(n) != CFBooleanGetTypeID(),n.floatValue.isFinite else {throw CraftError(path:key,reason:"Expected finite number")}
      return n.floatValue
    }
    func keys(_ operation:[String:Any],_ allowed:Set<String>) throws {
      try CraftError.require(Set(operation.keys).isSubset(of:allowed),"operation","Unknown field: \(Set(operation.keys).subtracting(allowed).sorted().joined(separator:", "))")
    }
    for (index,op) in operations.enumerated() {
      switch op["op"] as? String {
      case "upsert","remove":
        try keys(op,["op","collection","value","key"])
        guard let collection=op["collection"] as? String,arrays.contains(collection) else {throw CraftError(path:"operation[\(index)]",reason:"Unknown collection")}
        let idField=collection=="masses" ? "joint":"id"
        var values=document[collection] as? [[String:Any]] ?? []
        if op["op"] as? String=="upsert" {
          guard let value=op["value"] as? [String:Any],let id=value[idField] as? String,!id.isEmpty else {throw CraftError(path:collection,reason:"Record needs \(idField)")}
          if let i=values.firstIndex(where:{$0[idField] as? String==id}) {values[i]=value} else {values.append(value)}
        } else {
          guard let id=op["key"] as? String,let i=values.firstIndex(where:{$0[idField] as? String==id}) else {throw CraftError(path:collection,reason:"Cannot remove unknown record")}
          values.remove(at:i)
        }
        document[collection]=values
      case "anatomyAnchor":
        try keys(op,["op","key","source","point","maximumProjection"])
        let craft=try decodeDocument()
        guard let key=op["key"] as? String,let id=op["source"] as? String,let source=craft.anatomy.first(where:{$0.id==id}),let raw=op["point"] else {throw CraftError(path:"anatomyAnchor",reason:"Supply a key, current source ID and bind point")}
        let point=try JSONDecoder().decode(V3.self,from:JSONSerialization.data(withJSONObject:raw))
        let proposed=try source.anchor(id:key,at:point)
        let projection=try AnatomyCompiler.rebind([proposed],from:source,to:source,
          policy:AnatomyRebindPolicy(maximumProjection:try number(op,"maximumProjection",0.05)))
        try CraftError.require(projection.accepted,"anatomyAnchor.projection",projection.issues.map{"\($0.reason). \($0.recovery)"}.joined(separator:"; "))
        var anchors=craft.anatomyAnchors;anchors.removeAll{$0.id==key};anchors.append(projection.anchors[0])
        document["anatomyAnchors"]=try craftJSON(anchors)
      case "bindAnatomy":
        try keys(op,["op","key","anchor","kind","target","followNormal"])
        let craft=try decodeDocument()
        guard let key=op["key"] as? String,let anchorID=op["anchor"] as? String,let anchor=craft.anatomyAnchors.first(where:{$0.id==anchorID}),
          let target=op["target"] as? String,let rawKind=op["kind"] as? String,let kind=AnatomyBindingKind(rawValue:rawKind) else {throw CraftError(path:"bindAnatomy",reason:"Supply key, current anchor ID, kind and target ID")}
        if let value=op["followNormal"] as? NSNumber {try CraftError.require(CFGetTypeID(value)==CFBooleanGetTypeID(),"bindAnatomy.followNormal","Expected boolean")} else if op["followNormal"] != nil {throw CraftError(path:"bindAnatomy.followNormal",reason:"Expected boolean")}
        let binding=AnatomyBinding(id:key,anchor:anchorID,target:target,kind:kind,bindPosition:anchor.sourcePosition,bindNormal:anchor.sourceNormal,followNormal:op["followNormal"] as? Bool ?? false)
        var bindings=craft.anatomyBindings;bindings.removeAll{$0.id==key};bindings.append(binding);document["anatomyBindings"]=try craftJSON(bindings)
      case "rebindAnatomy":
        try keys(op,["op","key","allowStructuralChanges","maximumProjection","tolerance","elementMap"])
        guard let id=op["key"] as? String else {throw CraftError(path:"rebindAnatomy",reason:"Supply anatomy source key")}
        if let value=op["allowStructuralChanges"] as? NSNumber {try CraftError.require(CFGetTypeID(value)==CFBooleanGetTypeID(),"rebindAnatomy.allowStructuralChanges","Expected boolean")} else if op["allowStructuralChanges"] != nil {throw CraftError(path:"rebindAnatomy.allowStructuralChanges",reason:"Expected boolean")}
        let mapping=op["elementMap"] as? [String:String] ?? [:]
        if op["elementMap"] != nil {try CraftError.require(op["elementMap"] is [String:String],"rebindAnatomy.elementMap","Use an object mapping previous IDs to replacement IDs")}
        rebindPolicies[id]=AnatomyRebindPolicy(allowStructuralChanges:op["allowStructuralChanges"] as? Bool ?? false,
          maximumProjection:try number(op,"maximumProjection",0.05),tolerance:try number(op,"tolerance",0.0002),elementMap:mapping)
      case "editInterval":
        try keys(op,["op","key","edit"])
        var craft=try decodeDocument()
        guard let key=op["key"] as? String,let i=craft.phrases.firstIndex(where:{$0.id==key}),let raw=op["edit"] else {throw CraftError(path:"editInterval",reason:"Supply an existing phrase key and edit object")}
        try CraftSchema.validate(raw,definition:"motionIntervalEdit")
        let edit=try JSONDecoder().decode(MotionIntervalEdit.self,from:JSONSerialization.data(withJSONObject:raw))
        craft.phrases[i]=try edit.applying(to:craft.phrases[i]);document["phrases"]=try craftJSON(craft.phrases)
      case "set":
        try keys(op,["op","collection","value"])
        guard let collection=op["collection"] as? String,["refinement","arrangement","balance"].contains(collection),let value=op["value"] else {throw CraftError(path:"set",reason:"Choose refinement, arrangement or balance and supply a value")}
        if value is NSNull && collection != "refinement" {document.removeValue(forKey:collection)} else {document[collection]=value}
      case "capturePhrase":
        try keys(op,["op","key","duration","samples"])
        guard let key=op["key"] as? String,!key.isEmpty,creatureWorkshop.mode != "bind",creatureWorkshop.mode != "behavior" else {throw CraftError(path:"capturePhrase",reason:"Choose a named clip and supply a key")}
        let duration=try number(op,"duration",performanceDuration),n=try number(op,"samples",25)
        try CraftError.require((0.1...60).contains(duration) && n.rounded()==n && (2...121).contains(n),"capturePhrase","Duration 0.1…60; integer samples 2…121")
        candidate.craft=try decodeDocument();try candidate.validate()
        var samples:[PoseSample]=[],paths:[String:[ContactKey]]=[:]
        for i in 0..<Int(n) {
          let t=duration*Float(i)/Float(Int(n)-1),pose=candidate.authoredPose(clip:creatureWorkshop.mode,time:t,state:creatureWorkshop.actor)
          samples.append(PoseSample(time:t,poses:Dictionary(uniqueKeysWithValues:candidate.joints.map{($0.id,pose.poses[$0.id] ?? JointPose())})))
          for id in pose.contacts.keys.sorted() {let target=pose.contacts[id]!;paths[id,default:[]].append(ContactKey(time:t,position:target.position,planted:target.planted))}
        }
        // Sampling can straddle liftoff. Mark the interval unplanted when its
        // endpoints move; never store an invalid planted moving anchor.
        for id in paths.keys.sorted() {for i in 0..<(paths[id]!.count-1) {
          if length(paths[id]![i+1].position-paths[id]![i].position)>0.000001 {paths[id]![i].planted=false}
        }}
        let phrase=MotionPhrase(id:key,duration:duration,samples:samples,contacts:paths.keys.sorted().map{ContactPath(chain:$0,keys:paths[$0]!)},interpolation:.spline)
        var phrases=document["phrases"] as? [[String:Any]] ?? [];phrases.removeAll{$0["id"] as? String==key};phrases.append(try craftJSON(phrase) as! [String:Any]);document["phrases"]=phrases
      case "retimePhrase","mirrorPhrase","retargetPhrase":
        try keys(op,["op","key","newKey","duration","jointMap","chainMap","scale"])
        let craft=try decodeDocument()
        guard let key=op["key"] as? String,var phrase=craft.phrases.first(where:{$0.id==key}) else {throw CraftError(path:"phrase",reason:"Unknown phrase")}
        if op["op"] as? String=="retimePhrase" {
          let duration=try number(op,"duration");try CraftError.require((0.1...60).contains(duration),"retime","Duration 0.1…60")
          phrase=phrase.retimed(to:duration)
          // Retime references to this source together so arrangement timing stays
          // fixed while still covering the same portion of the source phrase.
          if var arrangement=craft.arrangement {
            let ratio=duration/craft.phrases.first(where:{$0.id==key})!.duration
            for i in arrangement.layers.indices where arrangement.layers[i].phrase==key {arrangement.layers[i].sourceStart *= ratio;arrangement.layers[i].sourceEnd *= ratio}
            document["arrangement"]=try craftJSON(arrangement)
          }
        } else {
          guard let newKey=op["newKey"] as? String,newKey != key,!craft.phrases.contains(where:{$0.id==newKey}),
            let mapping=op["jointMap"] as? [String:String],let chains=op["chainMap"] as? [String:String] else {throw CraftError(path:"retarget",reason:"Supply a new unused key and explicit jointMap/chainMap objects")}
          try CraftError.require(mapping.keys.allSatisfy{phrase.samples[0].poses[$0] != nil} && chains.keys.allSatisfy{id in phrase.contacts.contains{$0.chain==id}},"retarget.mapping","Unknown source joint or contact")
          if op["op"] as? String=="mirrorPhrase" {phrase=try phrase.mirrored(id:newKey,jointMap:mapping,chainMap:chains)}
          else {
            let scale=try number(op,"scale",1);try CraftError.require((0.1...10).contains(scale),"retarget.scale","Use 0.1…10")
            phrase=try phrase.mirrored(id:newKey,jointMap:mapping,chainMap:chains)
            phrase=try phrase.mirrored(id:newKey,jointMap:[:],chainMap:[:])
            phrase.samples=phrase.samples.map{s in var s=s;s.poses=s.poses.mapValues{p in var p=p;p.offset *= scale;return p};return s}
            phrase.contacts=phrase.contacts.map{c in var c=c;c.keys=c.keys.map{k in var k=k;k.position *= scale;return k};return c}
            phrase.references=phrase.references.map{r in var r=r;r.landmarks=r.landmarks.mapValues{$0*scale};return r}
          }
        }
        var phrases=document["phrases"] as? [[String:Any]] ?? [];phrases.removeAll{$0["id"] as? String==phrase.id};phrases.append(try craftJSON(phrase) as! [String:Any]);document["phrases"]=phrases
      case "fitSculpt":
        try keys(op,["op","key","part","controls","targets","maximumDisplacement","iterations","protect","maximumNormalChangeDegrees"])
        var craft=try decodeDocument()
        guard let key=op["key"] as? String,let part=op["part"] as? String,let rawControls=op["controls"] as? [[String:Any]],let rawTargets=op["targets"] as? [[String:Any]] else {throw CraftError(path:"fitSculpt",reason:"Supply a new layer key, part, controls and target/preservation points")}
        try CraftError.require(!craft.sculptLayers.contains{$0.id==key},"fitSculpt.layer","Use a new layer ID; vary existing layer opacity or restore its baseline before refitting")
        for control in rawControls {try keys(control,["center","radius"])}
        for target in rawTargets {try keys(target,["id","point","target","tolerance"])}
        let rawProtect=op["protect"] ?? []
        guard let protectionRecords=rawProtect as? [[String:Any]] else {throw CraftError(path:"fitSculpt.protect",reason:"Expected protection records")}
        for mask in protectionRecords {try keys(mask,["id","center","radius","innerFraction"])}
        let protect=try JSONDecoder().decode([SculptProtection].self,from:JSONSerialization.data(withJSONObject:protectionRecords))
        let controls=try JSONDecoder().decode([SculptControl].self,from:JSONSerialization.data(withJSONObject:rawControls))
        let targets=try JSONDecoder().decode([SculptTarget].self,from:JSONSerialization.data(withJSONObject:rawTargets))
        let count=try number(op,"iterations",24)
        try CraftError.require(count.rounded()==count && (1...64).contains(count),"fitSculpt.iterations","Use integer iterations 1…64")
        candidate.craft=craft
        let batches=try index==0 ? inspectionBatches(part:part):candidate.finish(candidate.compileBase().filter{$0.name==part})
        guard let batch=batches.first else {throw CraftError(path:"fitSculpt.part",reason:"Unknown part")}
        let fit=try SurfaceIntentFit.fit(id:key,part:part,mesh:batch.mesh,controls:controls,targets:targets,
          maximumDisplacement:try number(op,"maximumDisplacement",0.15),iterations:Int(count),protect:protect,
          maximumNormalChangeDegrees:try number(op,"maximumNormalChangeDegrees",180))
        craft.sculptLayers.append(fit.layer);document["sculptLayers"]=try craftJSON(craft.sculptLayers)
        sculptFits.append(["key":key,"iterations":fit.iterations,"residuals":try craftJSON(fit.residuals),"maximumDisplacement":fit.maximumDisplacement,
          "protectedVertices":fit.protectedVertices,"maximumProtectedDisplacement":fit.maximumProtectedDisplacement,"maximumNormalChangeDegrees":fit.maximumNormalChangeDegrees,"space":"compiled bind surface metres"])
      case "posedCorrective":
        try keys(op,["op","key","part","point","offset","radius","driver","axis","start","end","time","maximumProjection"])
        var craft=try decodeDocument()
        guard let key=op["key"] as? String,let part=op["part"] as? String,let driver=op["driver"] as? String,
          let rawPoint=op["point"],let rawOffset=op["offset"],creatureWorkshop.mode != "bind",creatureWorkshop.mode != "behavior"
          else {throw CraftError(path:"posedCorrective",reason:"Select a named motion clip and supply key, part, driver, posed point and offset")}
        let point=try JSONDecoder().decode(V3.self,from:JSONSerialization.data(withJSONObject:rawPoint))
        let offset=try JSONDecoder().decode(V3.self,from:JSONSerialization.data(withJSONObject:rawOffset))
        let time=try number(op,"time",creatureWorkshop.seconds),axis=try number(op,"axis"),radius=try number(op,"radius")
        try CraftError.require((0...performanceDuration).contains(time) && axis.rounded()==axis && (0...2).contains(axis),
          "posedCorrective.time","Use a time within the active clip and an integer axis 0…2")
        craft.correctives.removeAll{$0.id==key};candidate.craft=craft;try candidate.validate()
        guard let batch=try candidate.compile().first(where:{$0.name==part}) else {throw CraftError(path:"posedCorrective.part",reason:"Unknown compiled part")}
        let secondaryIDs=Set(candidate.resolvedSecondaryRig.nodes.map(\.id))
        try CraftError.require(!batch.skinJoints.contains(where:secondaryIDs.contains),"posedCorrective.part",
          "Select a body surface driven by rig joints; use guide controls for simulated groom or cloth")
        var lab=creatureWorkshop;try lab.seek(time,motion:candidate.motion ?? MotionParameters())
        let root=lab.root(candidate.motion ?? MotionParameters())
        let matrices=lab.evaluatedPose(source:candidate).matrices.mapValues{root*$0}
        let poses=candidate.authoredPose(clip:lab.mode,time:lab.seconds,state:lab.actor).poses
        let fields=candidate.resolvedCorrectives.filter{$0.field.part==part}.map{c->SurfaceEdit in
          var field=c.field;let w=c.activation(poses[c.driver] ?? JointPose());field.offset *= w;field.dilation *= w;return field
        }
        let corrected=DeformationAudit.corrected(batch.mesh,fields:fields)
        var corrective=PoseCorrective(id:key,driver:driver,axis:Int(axis),start:try number(op,"start"),end:try number(op,"end"),
          field:SurfaceEdit(id:key,part:part,center:.zero,radius:V3(repeating:radius)))
        try corrective.validate(joints:Set(candidate.joints.map(\.id)))
        let activation=corrective.activation(poses[driver] ?? JointPose())
        let fit=try PosedSurfaceFit.fit(id:key,part:part,mesh:corrected,weights:batch.skinWeights,
          palette:batch.skinJoints.map{matrices[$0] ?? matrix_identity_float4x4},rigid:matrices[part] ?? matrix_identity_float4x4,
          point:point,offset:offset,radius:radius,activation:activation,maximumProjection:try number(op,"maximumProjection",0.1))
        corrective.field=fit.field;craft.correctives.append(corrective)
        document["correctives"]=try craftJSON(craft.correctives)
        var row:[String:Any]=["key":key,"time":lab.seconds,"activation":activation,"bindCenter":try craftJSON(fit.field.center),
          "posedPoint":try craftJSON(fit.posedPoint),"correctedPoint":try craftJSON(fit.correctedPoint),
          "selectionDistance":fit.selectionDistance,"residual":fit.residual]
        if let anatomy=candidate.craft.anatomy.first(where:{$0.part==part}) {
          let region=anatomy.sample(at:fit.field.center);row["source"]=anatomy.id;row["element"]=region.element;row["region"]=region.region
        }
        posedFits.append(row)
      case "fitPose":
        try keys(op,["op","key","time","targets","freedoms","iterations","tolerance","requireConvergence"])
        var craft=try decodeDocument()
        guard let key=op["key"] as? String,let i=craft.phrases.firstIndex(where:{$0.id==key}),
          let ts=op["targets"] as? [[String:Any]],ts.allSatisfy({Set($0.keys)==Set(["joint","point","target","weight"])}),
          let fs=op["freedoms"] as? [[String:Any]],fs.allSatisfy({Set($0.keys)==Set(["joint","channel","minimum","maximum"])}) else {throw CraftError(path:"fitPose",reason:"Supply phrase key, targets and bounded freedoms")}
        let time=try number(op,"time"),iterations=try number(op,"iterations",32),tolerance=try number(op,"tolerance",0.002)
        try CraftError.require((0...craft.phrases[i].duration).contains(time) && iterations.rounded()==iterations && (1...80).contains(iterations),"fitPose","Use an in-range phrase time and 1…80 integer iterations")
        let targets=try JSONDecoder().decode([PoseTarget].self,from:JSONSerialization.data(withJSONObject:ts))
        let freedoms=try JSONDecoder().decode([PoseFreedom].self,from:JSONSerialization.data(withJSONObject:fs))
        try CraftError.require(freedoms.allSatisfy{craft.phrases[i].samples[0].poses[$0.joint] != nil},"fitPose","Author the selected channels' joints in this phrase first")
        candidate.craft=craft;try candidate.validate()
        if op["requireConvergence"] != nil {try CraftError.require((op["requireConvergence"] as? NSNumber).map{CFGetTypeID($0)==CFBooleanGetTypeID()} ?? false,"fitPose.requireConvergence","Expected boolean")}
        let fit=try PoseFitting.solve(joints:candidate.joints,poses:craft.phrases[i].pose(at:time),targets:targets,freedoms:freedoms,iterations:Int(iterations),tolerance:tolerance)
        if (op["requireConvergence"] as? Bool ?? true) && !fit.converged {throw CraftError(path:"fitPose.residual",reason:"Target remains \(fit.maximumError) m away after \(fit.iterations) iterations; enlarge selected channel bounds or revise targets")}
        craft.phrases[i].samples.removeAll{abs($0.time-time)<0.000001}
        craft.phrases[i].samples.append(PoseSample(time:time,poses:fit.poses));craft.phrases[i].samples.sort{$0.time<$1.time}
        document["phrases"]=try craftJSON(craft.phrases)
      case "footsteps":
        try keys(op,["op","key","steps","minimumSupport"])
        var craft=try decodeDocument()
        guard let key=op["key"] as? String,let i=craft.phrases.firstIndex(where:{$0.id==key}),let raw=op["steps"] as? [[String:Any]],
          raw.allSatisfy({Set($0.keys)==Set(["chain","lift","land","target","height"])}) else {throw CraftError(path:"footsteps",reason:"Supply an existing phrase key and steps")}
        let steps=try JSONDecoder().decode([AuthoredFootstep].self,from:JSONSerialization.data(withJSONObject:raw))
        let count=try number(op,"minimumSupport",1)
        try CraftError.require(count.rounded()==count && (0...32).contains(count),"footsteps.minimumSupport","Expected integer 0…32")
        let initial=Dictionary(uniqueKeysWithValues:craft.phrases[i].contacts.map{($0.chain,$0.keys[0].position)})
        craft.phrases[i].contacts=try FootstepPlan.compile(steps,initial:initial,duration:craft.phrases[i].duration,minimumSupport:Int(count))
        document["phrases"]=try craftJSON(craft.phrases)
      case "rigChain":
        try keys(op,["op","key","parent","points","reparent"])
        guard let key=op["key"] as? String,!key.isEmpty,let parent=op["parent"] as? String,let raw=op["points"],let children=op["reparent"] as? [String] else {throw CraftError(path:"rigChain",reason:"Supply key, parent, points and reparent array")}
        let points=try JSONDecoder().decode([V3].self,from:JSONSerialization.data(withJSONObject:raw))
        try CraftError.require((2...16).contains(points.count),"rigChain","Use 2…16 pivots")
        candidate.craft=try decodeDocument();let original=candidate.joints
        try CraftError.require(original.contains{$0.id==parent} && children.allSatisfy{id in original.contains{$0.id==id}},"rigChain","Unknown parent or child")
        var joints=candidate.craft.joints
        for (i,p) in points.enumerated() {
          let id=key+"-\(i)";try CraftError.require(!original.contains{$0.id==id},"rigChain","Joint \(id) already exists")
          joints.append(PartJoint(id:id,parent:i==0 ? parent:key+"-\(i-1)",pivot:p))
        }
        for id in children {var j=original.first{$0.id==id}!;j.parent=key+"-\(points.count-1)";joints.removeAll{$0.id==id};joints.append(j)}
        document["joints"]=try craftJSON(joints)
      default:throw CraftError(path:"operation[\(index)]",reason:"Unknown operation")
      }
    }
    candidate.craft=try decodeDocument()
    var correspondence:[[String:Any]]=[]
    try CraftError.require(rebindPolicies.keys.allSatisfy{id in studioSource.craft.anatomy.contains{$0.id==id} && candidate.craft.anatomy.contains{$0.id==id}},"rebindAnatomy","Recovery requires an existing and candidate source with the same ID")
    for source in candidate.craft.anatomy {
      guard let previous=studioSource.craft.anatomy.first(where:{$0.id==source.id}) else {continue}
      let changes=source.changes(from:previous)
      guard changes.rebuildRequired || rebindPolicies[source.id] != nil else {continue}
      let anchors=candidate.craft.anatomyAnchors.filter{anchor in anchor.source==source.id && studioSource.craft.anatomyAnchors.contains(anchor)}
      var row:[String:Any]=["source":source.id,"changes":try craftJSON(changes),"preservedAnchors":anchors.count]
      if !anchors.isEmpty {
        let result=try AnatomyCompiler.rebind(anchors,from:previous,to:source,policy:rebindPolicies[source.id] ?? AnatomyRebindPolicy())
        try CraftError.require(result.accepted,"anatomy.\(source.id).rebind",result.issues.map{"\($0.anchor): \($0.reason). \($0.recovery)"}.joined(separator:"; "))
        let ids=Set(anchors.map(\.id));candidate.craft.anatomyAnchors.removeAll{ids.contains($0.id)};candidate.craft.anatomyAnchors += result.anchors
        row["maximumProjectionMetres"]=result.maximumProjectionDistance;row["maximumFieldResidual"]=result.maximumFieldResidual
      }
      correspondence.append(row)
    }
    try candidate.validate();try applySource(candidate)
    return ["correspondence":correspondence,"posedFits":posedFits,"sculptFits":sculptFits]
  }

  /// Read the actual immutable compiled buffers used by this preview. Source
  /// compilation is transactional, so these are the current source's geometry;
  /// rerunning every field/groom compiler for an inspection is unnecessary.
  func inspectionBatches(part:String) throws -> [SceneBatch] {
    let selected=studioBatches.filter{$0.name==part}
    try CraftError.require(!selected.isEmpty,"report.part","Unknown part")
    return selected.map { batch in
      var mesh=Mesh()
      mesh.vertices=Array(UnsafeBufferPointer(start:batch.vertices.contents().assumingMemoryBound(to:Vertex.self),
        count:batch.vertices.length/MemoryLayout<Vertex>.stride))
      mesh.indices=Array(UnsafeBufferPointer(start:batch.indices.contents().assumingMemoryBound(to:UInt32.self),count:batch.indexCount))
      var result=SceneBatch(name:batch.name,mesh:mesh,instances:batch.sourceInstances)
      result.skinJoints=batch.skinJoints
      if let buffer=batch.skinWeights {
        result.skinWeights=Array(UnsafeBufferPointer(start:buffer.contents().assumingMemoryBound(to:SkinWeight.self),count:mesh.vertices.count))
      }
      return result
    }
  }

  func craftReport(samples:Int,part:String?=nil,start:Float=0,end:Float?=nil) throws -> [String:Any] {
    try CraftError.require((2...121).contains(samples),"report.samples","Use 2…121 samples")
    let end=end ?? performanceDuration
    try CraftError.require(start.isFinite && end.isFinite && start>=0 && end>=start && end<=performanceDuration,
      "report.interval","Use 0 ≤ start ≤ end ≤ clip duration; equal endpoints inspect one pose")
    let batches:[SceneBatch]=try part.map{try inspectionBatches(part:$0)} ?? []
    let player=SecondaryPosePlayer();var rows:[[String:Any]]=[]
    var last:[String:V3]=[:],velocity:[String:V3]=[:],maximumSpeed:Float=0,maximumAcceleration:Float=0,maximumOutside:Float=0
    // Include liftoff/landing boundaries and adjacent fixed ticks so a coarse
    // review grid cannot silently miss a short unsupported recovery interval.
    let firstTick=Int((start*60).rounded()),lastTick=Int((end*60).rounded())
    var ticks=Set<Int>()
    let tickStart:Float=start*60, tickSpan:Float=(end-start)*60
    for i in 0..<samples {
      let fraction:Float=Float(i)/Float(samples-1)
      ticks.insert(Int((tickStart+tickSpan*fraction).rounded()))
    }
    if let a=studioSource.craft.arrangement,a.clip==creatureWorkshop.mode {
      for layer in a.layers {
        guard let phrase=studioSource.craft.phrases.first(where:{$0.id==layer.phrase}) else {continue}
        for path in phrase.contacts {for key in path.keys where key.time>=layer.sourceStart && key.time<=layer.sourceEnd {
          let t=layer.start+(key.time-layer.sourceStart)/(layer.sourceEnd-layer.sourceStart)*(layer.end-layer.start)
          for delta in [-1,0,1] {
            let event=Int((t*60).rounded())+delta
            if event>=firstTick && event<=lastTick {ticks.insert(event)}
          }
        }}
      }
    }
    try CraftError.require(ticks.count<=721,"report.events","More than 721 event samples; simplify the active contact paths")
    var previousTime:Float=0,previousDT:Float=0
    for tick in ticks.sorted() {
      var lab=creatureWorkshop;try lab.seek(Float(tick)/60,motion:studioSource.motion ?? MotionParameters())
      let dt=lab.seconds-previousTime
      let result=lab.evaluatedPose(source:studioSource),root=lab.root(studioSource.motion ?? MotionParameters())
      let matrices=result.matrices.mapValues{root*$0}
      var positions:[String:V3]=[:]
      for j in studioSource.joints {positions[j.id]=CraftMath.point(matrices[j.id] ?? matrix_identity_float4x4,j.pivot)}
      var speed:Float=0,acceleration:Float=0
      for id in positions.keys.sorted() {
        let p=positions[id]!
        if let before=last[id] {
          let v=(p-before)/dt;speed=max(speed,length(v))
          if let old=velocity[id] {acceleration=max(acceleration,length(v-old)/max(0.000001,(dt+previousDT)*0.5))};velocity[id]=v
        };last[id]=p
      }
      previousTime=lab.seconds;previousDT=dt
      maximumSpeed=max(maximumSpeed,speed);maximumAcceleration=max(maximumAcceleration,acceleration)
      let anchors=result.contacts.filter(\.planted).map{CraftMath.point(root,$0.actual)}
      let support=BodySupport.inspect(masses:studioSource.craft.masses,matrices:matrices,anchors:anchors)
      maximumOutside=max(maximumOutside,support.outsideDistance)
      var deformation:[[String:Any]]=[]
      if !batches.isEmpty {
        let pose=secondaryPose(lab:lab,player:player)
        let authored=studioSource.authoredPose(clip:lab.mode,time:lab.seconds,state:lab.actor).poses
        for batch in batches {
          let fields=studioSource.resolvedCorrectives.filter{$0.field.part==batch.name}.map{c->SurfaceEdit in var f=c.field;let w=lab.mode=="bind" ? 0:c.activation(authored[c.driver] ?? JointPose());f.offset *= w;f.dilation *= w;return f}
          let corrected=DeformationAudit.corrected(batch.mesh,fields:fields)
          let posed=DeformationAudit.deformed(corrected,weights:batch.skinWeights,palette:batch.skinJoints.map{pose[$0] ?? matrix_identity_float4x4},rigid:pose[batch.name] ?? matrix_identity_float4x4)
          deformation.append(["part":batch.name,"metrics":try craftJSON(DeformationAudit.inspect(rest:batch.mesh,posed:posed))])
        }
      }
      rows.append(["time":lab.seconds,"joints":try craftJSON(positions),"contacts":try craftJSON(result.contacts),
        "support":try craftJSON(support),"maximumJointSpeed":speed,"maximumJointAcceleration":acceleration,"deformation":deformation])
    }
    return ["revision":sourceRevision,"clip":creatureWorkshop.mode,"duration":performanceDuration,"start":start,"end":end,"samples":rows,
      "maximumJointSpeed":maximumSpeed,"maximumJointAcceleration":maximumAcceleration,"maximumOutsideSupportMetres":maximumOutside,
      "hasMassModel": !studioSource.craft.masses.isEmpty,
      "references":try craftJSON(studioSource.craft.phrases.flatMap(\.references)),
      "scope":"Uniform review samples plus liftoff/landing events and adjacent 60 Hz ticks. Sampled final contact-constrained poses, user-authored lumped masses, static support polygon and finite-difference joint trajectories. Surface reports include corrective fields and four-weight skinning. No force/momentum certification, continuous-time collision or artistic-quality score. Retargeting maps joint identities/translations; it is not automatic anatomy inference."]
  }
}
