import CryptoKit
import FieldCompiler
import FieldCore
import FieldEngine
import Foundation
import simd

extension WorkshopRenderer {
  var sourceJSONObject:Any {
    if let cached=studioSourceObjectCache {return cached}
    let value=(try? JSONSerialization.jsonObject(with:JSONEncoder().encode(studioSource))) ?? [:]
    studioSourceObjectCache=value;return value
  }
  var sourceRevision: String {
    if let cached=studioRevisionCache {return cached}
    let encoder=JSONEncoder();encoder.outputFormatting=[.sortedKeys]
    let revision=SHA256.hash(data:try! encoder.encode(studioSource)).map {String(format:"%02x",$0)}.joined()
    studioRevisionCache=revision;return revision
  }
  func authoringSnapshot() throws -> [String:Any] {
    let source=sourceJSONObject
    return ["revision":sourceRevision,"source":source,"baseGeometryReused":studioBaseCacheHit,
      "buildMilliseconds":studioBuildMilliseconds,"reusedBatches":studioReusedBatches,"rebuiltBatches":studioRebuiltBatches,
      "operations":["author: atomic replacement of surfaceEdits, surfaceLayers, guideOffsets, guideRadii, guideFrames, secondary, performance, contactOffsets, collisionBodies; expectedRevision required",
        "surfaceProbe: bind-space nearest surface point and bounds",
        "rehearsal: slopeX, slopeZ, stepHeight, stepZ; view and solving booleans",
        "poseReport: sample final contact-constrained motion without changing the scene",
        "secondary: merge enabled, gravity, damping, followCompliance, wind, substeps, iterations, edgeContacts, friction",
        "secondaryReport: sample deterministic guide dynamics without changing the scene",
        "surfaceContacts: audit all deformed vertices of secondary surfaces, or a named part; samples 1 (current time) to 121 (whole clip)"],
      "surfaceEditSpace":"bind coordinates in metres; ordered compact deformation fields",
      "surfaceEditLimits":["layers":64,"radiusMin":0.005,"radiusMax":20],
      "surfaceLayerSchema":["space":"bind metres, authored RGB tint; ordered compact ellipsoid coverage",
        "fields":"id, part, center[3], radius[3], tint[3], amount[0...1], frequency[0.1...1000], roughness[-1...1], metallic[-1...1], relief[-0.03...0.03] metres, pattern[0...3], seed[-1000...1000]",
        "patterns":"0 uniform, 1 mottling, 2 noise contours, 3 directional grain",
        "limits":"64 per source, 16 per part; -1 preserves recipe roughness/metallic; no geometry rebuild"],
      "secondaryRig":try JSONSerialization.jsonObject(with:JSONEncoder().encode(studioSource.resolvedSecondaryRig)),
      "collisionBodies":try JSONSerialization.jsonObject(with:JSONEncoder().encode(studioSource.resolvedColliders)),
      "contactChains":studioSource.animation?.contactChains.map(\.id) ?? []]
  }
  func author(_ request: [String:Any]) throws {
    guard Set(request.keys).isSubset(of:["id","action","expectedRevision","surfaceEdits","performance","contactOffsets","collisionBodies","secondary","guideOffsets","guideRadii","guideFrames","surfaceLayers","craft"]),
      request["expectedRevision"] as? String == sourceRevision,
      request.keys.contains(where:{["surfaceEdits","performance","contactOffsets","collisionBodies","secondary","guideOffsets","guideRadii","guideFrames","surfaceLayers","craft"].contains($0)})
    else {throw RuntimeError.message("Author transaction needs current expectedRevision and supported fields")}
    var candidate=studioSource
    if let craft=request["craft"] {
      try CraftSchema.validate(craft)
      candidate.craft=try JSONDecoder().decode(CreatureCraft.self,from:JSONSerialization.data(withJSONObject:craft))
    }
    func decode<T:Decodable>(_ type:T.Type,_ key:String)throws->T {
      try JSONDecoder().decode(type,from:JSONSerialization.data(withJSONObject:request[key]!))
    }
    if request["surfaceEdits"] != nil {
      guard let edits=request["surfaceEdits"] as? [[String:Any]],edits.allSatisfy({Set($0.keys).isSubset(of:["id","part","targets","center","radius","offset","dilation"])}) else {throw RigError.invalid}
      candidate.surfaceEdits=try decode([SurfaceEdit].self,"surfaceEdits")
    }
    if request["performance"] is NSNull {candidate.performance=nil}
    else if request["performance"] != nil {candidate.performance=try decode(PerformanceScore.self,"performance")}
    if request["contactOffsets"] != nil {candidate.contactOffsets=try decode([String:V3].self,"contactOffsets")}
    if request["collisionBodies"] is NSNull {candidate.collisionBodies=nil}
    else if request["collisionBodies"] != nil {candidate.collisionBodies=try decode([RigCapsule].self,"collisionBodies")}
    if request["secondary"] != nil {
      guard let settings=request["secondary"] as? [String:Any],Set(settings.keys).isSubset(of:["enabled","gravity","damping","followCompliance","wind","substeps","iterations","edgeContacts","friction"]) else {throw RigError.invalid}
      candidate.secondary=try decode(SecondarySettings.self,"secondary")
    }
    if request["guideFrames"] != nil {candidate.guideFrames=try decode([String:GuideFrameMode].self,"guideFrames")}
    if request["guideRadii"] != nil {candidate.guideRadii=try decode([String:Float].self,"guideRadii")}
    if request["guideOffsets"] != nil {candidate.guideOffsets=try decode([String:V3].self,"guideOffsets")}
    if request["surfaceLayers"] != nil {
      guard let layers=request["surfaceLayers"] as? [[String:Any]],layers.allSatisfy({Set($0.keys).isSubset(of:["id","part","center","radius","tint","amount","frequency","roughness","metallic","relief","pattern","seed"])}) else {throw RigError.invalid}
      candidate.surfaceLayers=try decode([SurfaceLayer].self,"surfaceLayers")
    }
    try candidate.validate()
    try applySource(candidate)
  }
  func surfaceProbe(part:String, point:V3, radius:Float) throws -> [String:Any] {
    guard point.x.isFinite,point.y.isFinite,point.z.isFinite,radius.isFinite,(0.001...20).contains(radius),
      let batch=studioBatches.first(where:{$0.name==part}) else {throw RigError.invalid}
    let vertices=batch.vertices.contents().bindMemory(to:Vertex.self,capacity:batch.vertices.length/MemoryLayout<Vertex>.stride)
    var nearest=V3.zero,normal=V3.zero,lo=V3(repeating:Float.greatestFiniteMagnitude),hi = -lo
    var distance=Float.greatestFiniteMagnitude,count=0
    for i in 0..<batch.vertices.length/MemoryLayout<Vertex>.stride {
      let v=vertices[i],p=V3(v.position.x,v.position.y,v.position.z),d=length(p-point)
      lo=simd_min(lo,p);hi=simd_max(hi,p)
      if d<radius {count+=1}
      if d<distance {distance=d;nearest=p;normal=V3(v.normal.x,v.normal.y,v.normal.z)}
    }
    func a(_ v:V3)->[Float] {[v.x,v.y,v.z]}
    return ["part":part,"space":"bind","point":a(nearest),"normal":a(normal),"distance":distance,
      "verticesInRadius":count,"bounds":[a(lo),a(hi)],"revision":sourceRevision]
  }
  func editRehearsal(_ values:[String:Any]) throws {
    guard Set(values.keys).isSubset(of:["id","action","slopeX","slopeZ","stepHeight","stepZ","view","solving"]) else {throw RigError.invalid}
    var lab=creatureWorkshop
    for key in ["slopeX","slopeZ","stepHeight","stepZ"] where values[key] != nil {
      guard let n=values[key] as? NSNumber,CFGetTypeID(n) != CFBooleanGetTypeID() else {throw RigError.invalid}
      switch key {
      case "slopeX":lab.rehearsal.slopeX=n.floatValue
      case "slopeZ":lab.rehearsal.slopeZ=n.floatValue
      case "stepHeight":lab.rehearsal.stepHeight=n.floatValue
      default:lab.rehearsal.stepZ=n.floatValue
      }
    }
    for key in ["view","solving"] where values[key] != nil {
      guard let n=values[key] as? NSNumber,CFGetTypeID(n)==CFBooleanGetTypeID() else {throw RigError.invalid}
      if key=="view" {lab.contactView=n.boolValue} else {lab.contactSolving=n.boolValue}
    }
    try lab.validate(source:studioSource);creatureWorkshop=lab
  }
  func poseReport(start:Float,end:Float,samples:Int) throws -> [String:Any] {
    guard start.isFinite,end.isFinite,start>=0,end<=60,end>=start,(2...241).contains(samples) else {throw RigError.invalid}
    var plantedClearance:Float=0
    var collisions:[[String:Any]]=[]
    var rows:[[String:Any]]=[],worst:Float=0,minClearance=Float.greatestFiniteMagnitude,slip:Float=0
    var previous:[String:ContactObservation]=[:],previousTime:Float=start
    for i in 0..<samples {
      let t=start+(end-start)*Float(i)/Float(samples-1)
      var lab=creatureWorkshop
      try lab.seek(t,motion:studioSource.motion ?? MotionParameters())
      let result=lab.evaluatedPose(source:studioSource)
      let root=lab.root(studioSource.motion ?? MotionParameters())
      func world(_ v:V3)->V3 {let p=root*SIMD4(v,1);return V3(p.x,p.y,p.z)}
      let scale=length(V3(root.columns.1.x,root.columns.1.y,root.columns.1.z))
      let observations=result.contacts.map {c in
        var c=c;c.target=world(c.target);c.actual=world(c.actual);c.knee=world(c.knee)
        let n=root*SIMD4(c.normal,0);c.normal=normalize(V3(n.x,n.y,n.z))
        c.residual *= scale;c.clearance *= scale;c.correction *= scale;return c
      }
      for c in observations {
        if c.planted {plantedClearance=max(plantedClearance,abs(c.clearance))}
        worst=max(worst,c.residual);minClearance=min(minClearance,c.clearance)
        if let p=previous[c.id],p.planted,c.planted,t>previousTime {
          slip=max(slip,length(c.actual-p.actual)/(t-previousTime))
        }
        previous[c.id]=c
      }
      let bodies=studioSource.resolvedColliders.map {$0.transformed(root*(result.matrices[$0.joint] ?? matrix_identity_float4x4))}
      let hits=RigCollision.inspect(bodies,height:lab.rehearsal.height)
      for hit in hits where hit.separation < 0 {collisions.append(["time":t,"a":hit.a,"b":hit.b,"penetration":-hit.separation])}
      previousTime=t
      rows.append(["time":t,"contacts":try JSONSerialization.jsonObject(with:JSONEncoder().encode(observations))])
    }
    return ["revision":sourceRevision,"samples":rows,"collisions":collisions,"maximumReachErrorMetres":worst,
      "minimumSoleClearanceMetres":minClearance.isFinite && minClearance != Float.greatestFiniteMagnitude ? minClearance:0,
      "maximumPlantedSpeedMetresPerSecond":slip,"maximumPlantedClearanceMetres":plantedClearance,
      "scope":"Final rendered contact chains. Kinematic reach/contact constraints, not dynamics or whole-body collision certification."]
  }
}
