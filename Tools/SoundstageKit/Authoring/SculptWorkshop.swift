import CryptoKit
import FieldCore
import FieldCompiler
import FieldEngine
import Foundation
import MetalKit
import simd

/// Hold the framing/placement basis while comparing changing geometry. This is
/// study state, never character source. Orbit controls still work around the
/// same centre, and the light rig retains its original scale and position.
struct SculptReviewFrame:Codable {
  var source:String
  var minimum:V3
  var maximum:V3
  var center:V3
  func validate(source id:String) throws {
    try CraftError.require(source==id && CraftMath.finite(minimum,limit:100) && CraftMath.finite(maximum,limit:100)
      && CraftMath.finite(center,limit:100) && all(maximum .> minimum) && length(maximum-minimum)<100,
      "sculptFrameLock","Review frame must belong to this source and contain finite ordered bounds")
  }
}
struct SculptOverlay:Codable {
  var part:String
  var protect:[SculptProtection]
  var controls:[SculptControl]
  func validate(parts:Set<String>) throws {
    try CraftError.require(parts.contains(part) && protect.count<=32 && controls.count<=12,"sculptOverlay","Use one existing part, at most 32 protection volumes and 12 controls")
    for mask in protect {try mask.validate()}
    for c in controls {try CraftError.require(CraftMath.finite(c.center,limit:100) && c.radius.isFinite && (0.02...3).contains(c.radius),"sculptOverlay.control","Use finite bind coordinates and radii 0.02…3 m")}
  }
}

/// A snapshot of the very same items/matrices sent to the engine renderer.
/// Only the latest frame is retained; stale selection requests reject explicitly.
struct SculptFrame {
  var token:String
  var revision:String
  var vp:simd_float4x4
  var items:[RenderItem]
  var mode:String
}
extension WorkshopRenderer {
  @discardableResult func frameVisibleSurface(part:String?=nil) throws -> [String:Any] {
    try CraftError.require(paused,"frameVisible.paused","Pause before framing the current deformed surface")
    let names=Set(studioBatches.map(\.name))
    let items=renderItems().filter{names.contains($0.batch.name) && (part==nil || $0.batch.name==part)}
    try CraftError.require(!items.isEmpty,"frameVisible.parts","No requested visible subject surface; unhide it or clear isolation")
    let surfaces=sculptSurfaces(items)
    var lo=V3(repeating:Float.greatestFiniteMagnitude),hi = -lo
    for (_,_,mesh,_) in surfaces {for v in mesh.vertices {let p=V3(v.position.x,v.position.y,v.position.z);lo=simd_min(lo,p);hi=simd_max(hi,p)}}
    try CraftError.require(CraftMath.finite(lo,limit:1000) && CraftMath.finite(hi,limit:1000) && length(hi-lo)>0.00001,"frameVisible.bounds","Visible geometry has invalid bounds")
    let center=(lo+hi)/2,direction=V3(sin(orbit)*cos(orbitElevation),sin(orbitElevation),cos(orbit)*cos(orbitElevation))
    let right=normalize(cross(V3(0,1,0),direction)),up=cross(direction,right)
    var distance:Float=0
    for (_,_,mesh,_) in surfaces {for v in mesh.vertices {
      let p=V3(v.position.x,v.position.y,v.position.z)-center
      distance=max(distance,dot(p,direction)+max(abs(dot(p,right))/(tan(1.05/2)*Float(16)/9*0.85),abs(dot(p,up))/(tan(1.05/2)*0.85)))
    }}
    // Keep the scene placement and light rig unchanged. Only the camera target
    // and distance use visible geometry, with no hidden recipe/motion envelope.
    let frame=SculptReviewFrame(source:studioSource.id,minimum:studioBounds.min,maximum:studioBounds.max,center:center)
    try frame.validate(source:studioSource.id)
    sculptReviewFrame=frame
    inspectionCamera=nil
    orbitDistance=boundedStudioDistance((distance+length(hi-lo)*0.01)/studioUnit)
    return ["parts":surfaces.map{$0.0},"minimum":[lo.x,lo.y,lo.z],"maximum":[hi.x,hi.y,hi.z],"heightMetres":hi.y-lo.y,
      "cameraDistanceMetres":orbitDistance*studioUnit,"limits":"Current deformed mesh bounds; material alpha and procedural wind are not included. Framing is held until the comparison frame is unlocked."]
  }
  func editSculptOverlay(_ request:[String:Any]) throws {
    try CraftError.require(Set(request.keys).isSubset(of:["id","action","expectedRevision","value"])
      && request["expectedRevision"] as? String==sourceRevision,"sculptOverlay.revision","Supply the current source revision")
    if request["value"] is NSNull {sculptOverlay=nil;return}
    guard let raw=request["value"] as? [String:Any],Set(raw.keys).isSubset(of:["part","protect","controls"]),
      let masks=raw["protect"] as? [[String:Any]],let controls=raw["controls"] as? [[String:Any]],
      masks.allSatisfy({Set($0.keys).isSubset(of:["id","center","radius","innerFraction"])}),
      controls.allSatisfy({Set($0.keys).isSubset(of:["center","radius"])}),
      let value=try? JSONDecoder().decode(SculptOverlay.self,from:JSONSerialization.data(withJSONObject:raw))
      else {throw CraftError(path:"sculptOverlay.value",reason:"Supply null or {part,protect:[ellipsoids],controls:[{center,radius}]}")}
    try value.validate(parts:Set(studioBatches.map(\.name)))
    sculptOverlay=value
  }
  func overlayBatch(_ original:GPUBatch)->GPUBatch {
    guard let overlay=sculptOverlay,overlay.part==original.name else {return original}
    let key=sourceRevision+"/"+original.name
    if let cached=sculptOverlayCache,cached.0==key {return cached.1}
    var vertices=Array(UnsafeBufferPointer(start:original.vertices.contents().assumingMemoryBound(to:Vertex.self),count:original.vertices.length/MemoryLayout<Vertex>.stride))
    for i in vertices.indices {
      let v=vertices[i].position,p=V3(v.x,v.y,v.z),retention=overlay.protect.reduce(Float(1)){$0*$1.retention(p)}
      let influence=overlay.controls.reduce(Float(0)){value,c in
        let r=length(p-c.center)/c.radius
        return max(value,overlay.protect.isEmpty ? pow(max(0,1-r*r),3):1-CraftMath.smooth(r))
      }
      let base=mix(V3(repeating:0.57),V3(0.9,0.22,0.025),t:influence)
      vertices[i].color=SIMD4(mix(base,V3(0.025,0.3,0.85),t:1-retention),1)
    }
    var batch=original
    batch.vertices=graphics.device.makeBuffer(bytes:vertices,length:original.vertices.length,options:.storageModeShared)!
    sculptOverlayCache=(key,batch)
    return batch
  }
  func lockSculptFrame(_ request:[String:Any]) throws {
    try CraftError.require(Set(request.keys).isSubset(of:["id","action","enabled","expectedRevision"])
      && request["expectedRevision"] as? String==sourceRevision,"sculptFrameLock.revision","Supply the current source revision")
    guard let value=request["enabled"] as? NSNumber,CFGetTypeID(value)==CFBooleanGetTypeID() else {throw CraftError(path:"sculptFrameLock.enabled",reason:"Expected boolean")}
    try CraftError.require(paused,"sculptFrameLock.paused","Pause before establishing a comparison frame")
    let distance=orbitDistance*studioUnit
    if value.boolValue {
      if sculptReviewFrame==nil {sculptReviewFrame=SculptReviewFrame(source:studioSource.id,minimum:studioBounds.min,maximum:studioBounds.max,center:studioCenter)}
    } else {sculptReviewFrame=nil}
    orbitDistance=boundedStudioDistance(distance/studioUnit)
  }
  func rememberSculptFrame(_ input:RenderFrame) {
    // Work only when a capture was requested. Normal live frames do not hash
    // matrices or copy buffers for an authoring-only observation.
    guard graphics.captureRequest != nil else {return}
    let names=Set(studioBatches.map(\.name))
    let selected=input.items.filter{names.contains($0.batch.name)}
    var bytes=Data(sourceRevision.utf8)
    func append<T>(_ value:T) {var v=value;withUnsafeBytes(of:&v){bytes.append(contentsOf:$0)}}
    append(input.uniforms.viewProjection)
    for item in selected {
      bytes.append(contentsOf:item.batch.name.utf8)
      append((item.instance ?? item.batch.sourceInstances[0]).model)
      for m in item.skinPalette {append(m)}
      for c in item.correctives {append(c)}
    }
    let token=SHA256.hash(data:bytes).map{String(format:"%02x",$0)}.joined()
    sculptFrame=SculptFrame(token:token,revision:sourceRevision,vp:input.uniforms.viewProjection,items:selected,mode:creatureWorkshop.mode)
  }
  var sculptFrameMetadata:[String:Any] {
    guard let f=sculptFrame else {return [:]}
    return ["token":f.token,"revision":f.revision,"size":[1920,1080],"mode":f.mode,
      "coordinateConvention":"top-left pixel centres; bind coordinates in metres",
      "parts":f.items.map{$0.batch.name}]
  }
  func sculptSurfaces(_ items:[RenderItem])->[(String,Mesh,Mesh,Bool)] {
    var surfaces:[(String,Mesh,Mesh,Bool)]=[]
    let secondary=Set(studioSource.resolvedSecondaryRig.nodes.map(\.id))
    for item in items {
      let batch=item.batch
      var rest=Mesh()
      rest.vertices=Array(UnsafeBufferPointer(start:batch.vertices.contents().assumingMemoryBound(to:Vertex.self),count:batch.vertices.length/MemoryLayout<Vertex>.stride))
      rest.indices=Array(UnsafeBufferPointer(start:batch.indices.contents().assumingMemoryBound(to:UInt32.self),count:batch.indexCount))
      let weights=batch.skinWeights.map{Array(UnsafeBufferPointer(start:$0.contents().assumingMemoryBound(to:SkinWeight.self),count:rest.vertices.count))} ?? []
      // Correctives are source fields evaluated with the captured activation.
      var corrected=rest
      for c in item.correctives {
        let field=SurfaceEdit(id:"captured",part:batch.name,center:V3(c.center.x,c.center.y,c.center.z),radius:V3(c.radius.x,c.radius.y,c.radius.z),offset:V3(c.offset.x,c.offset.y,c.offset.z),dilation:V3(c.dilation.x,c.dilation.y,c.dilation.z))
        corrected=DeformationAudit.corrected(corrected,fields:[field])
      }
      let deformed=DeformationAudit.deformed(corrected,weights:weights,palette:item.skinPalette)
      let model=(item.instance ?? batch.sourceInstances[0]).model
      let posed=DeformationAudit.deformed(deformed,weights:[],palette:[],rigid:model)
      surfaces.append((batch.name,rest,posed,batch.skinJoints.contains(where:secondary.contains)))
    }
    return surfaces
  }
  func sculptProbe(_ request:[String:Any]) throws -> [String:Any] {
    try CraftError.require(Set(request.keys).isSubset(of:["id","action","frame","expectedRevision","pixels","part","explain"]),"pick.request","Unknown argument")
    if let explain=request["explain"] {
      try CraftError.require((explain as? NSNumber).map{CFGetTypeID($0)==CFBooleanGetTypeID()} ?? false,"pick.explain","Expected boolean")
    }
    guard let f=sculptFrame,let token=request["frame"] as? String,token==f.token,
      request["expectedRevision"] as? String==sourceRevision,f.revision==sourceRevision,
      let pixels=request["pixels"] as? [[NSNumber]],(1...64).contains(pixels.count),pixels.allSatisfy({$0.count==2 && $0.allSatisfy{CFGetTypeID($0) != CFBooleanGetTypeID() && $0.floatValue.isFinite}})
      else {throw CraftError(path:"pick.frame",reason:"Capture an actual render, supply its sculptFrame token/current revision and 1…64 pixel pairs; source changes require a new capture")}
    let expectedPart=request["part"] as? String
    let surfaces=sculptSurfaces(f.items)
    var hits:[[String:Any]]=[]
    for pixel in pixels {
      let ray=try SurfacePicking.ray(pixel:SIMD2(pixel[0].floatValue,pixel[1].floatValue),size:SIMD2(1920,1080),inverseVP:f.vp.inverse)
      var closest:(String,SurfacePicking.Hit,Bool)?
      for (name,rest,posed,secondary) in surfaces {
        if let hit=SurfacePicking.hit(rest:rest,posed:posed,origin:ray.origin,direction:ray.direction),hit.distance < (closest?.1.distance ?? .greatestFiniteMagnitude) {closest=(name,hit,secondary)}
      }
      guard let (name,h,isSecondary)=closest else {throw CraftError(path:"pick.miss",reason:"Pixel \(pixel) hits no visible subject surface; select inside the silhouette")}
      try CraftError.require(expectedPart==nil || expectedPart==name,"pick.occluded","Pixel hits \(name), not \(expectedPart ?? ""); hidden surfaces cannot be selected through foreground geometry")
      func a(_ v:V3)->[Float] {[v.x,v.y,v.z]}
      var row:[String:Any]=["pixel":pixel,"part":name,"point":a(h.bindPosition),"normal":a(h.bindNormal),
        "worldPoint":a(h.position),"worldNormal":a(h.normal),"triangle":h.triangle,"barycentric":a(h.barycentric),
        "triangleEdgeMetres":h.edgeMetres,"guideDeformed":isSecondary]
      if let anatomy=studioSource.craft.anatomy.first(where:{$0.part==name}) {
        let sample=anatomy.sample(at:h.bindPosition)
        row["source"]=anatomy.id;row["element"]=sample.element;row["region"]=sample.region
        if request["explain"] as? Bool == true {
          row["explanation"]=try JSONSerialization.jsonObject(with:JSONEncoder().encode(anatomy.explain(at:h.bindPosition)))
        }
      }
      hits.append(row)
    }
    return ["frame":token,"revision":sourceRevision,"hits":hits,"space":"bind metres",
      "limits":"Triangle selection includes captured skinning and correctives. Groom alpha, procedural wind and material bump are not geometric surfaces. Use stripped clay for body sculpting."]
  }
}
