import simd

/// Interior collision queries for the bilinear proxy, not rendered triangles.
/// Coordinate minimization is bounded and uses several deterministic starts;
/// it does not certify global clearance for an arbitrarily folded saddle.
private struct ContactPatchShape {
  var p:[V3]
  var radius:SIMD4<Float>
  var low:V3
  var high:V3
  init(_ positions:[V3],patch:SecondaryPatch,rig:SecondaryRig) {
    p=(0..<4).map{positions[patch.nodes[$0]]}
    radius=SIMD4(rig.nodes[patch.nodes[0]].radius,rig.nodes[patch.nodes[1]].radius,
      rig.nodes[patch.nodes[2]].radius,rig.nodes[patch.nodes[3]].radius)
    let padding=V3(repeating:radius.max())
    low=simd_min(simd_min(p[0],p[1]),simd_min(p[2],p[3]))-padding
    high=simd_max(simd_max(p[0],p[1]),simd_max(p[2],p[3]))+padding
  }
  func point(_ w:SIMD4<Float>)->V3 {p[0]*w.x+p[1]*w.y+p[2]*w.z+p[3]*w.w}
  func overlaps(_ body:RigCapsule)->Bool {
    let padding=V3(repeating:body.radius),a=simd_min(body.a,body.b)-padding,b=simd_max(body.a,body.b)+padding
    return (0..<3).allSatisfy{high[$0]>=a[$0] && low[$0]<=b[$0]}
  }
  func normal(_ uv:SIMD2<Float>)->V3 {
    let u=(p[1]-p[0])*(1-uv.y)+(p[3]-p[2])*uv.y
    let v=(p[2]-p[0])*(1-uv.x)+(p[3]-p[1])*uv.x
    let n=cross(u,v)
    return length_squared(n)>1e-12 ? normalize(n):V3(0,1,0)
  }
  func closest(_ body:RigCapsule,patch:SecondaryPatch)->(uv:SIMD2<Float>,weights:SIMD4<Float>,point:V3,t:Float,depth:Float) {
    var bestUV=SIMD2<Float>(repeating:0.5),bestDepth = -Float.greatestFiniteMagnitude,bestT:Float=0
    for seed in [SIMD2<Float>(0.5,0.5),SIMD2(0,0),SIMD2(1,1),SIMD2(0,1),SIMD2(1,0)] {
      var uv=seed
      for _ in 0..<6 {
        let old=uv
        let a=p[0]+(p[2]-p[0])*uv.y,b=p[1]+(p[3]-p[1])*uv.y
        let ra=radius.x+(radius.z-radius.x)*uv.y,rb=radius.y+(radius.w-radius.y)*uv.y
        uv.x=RigCollision.taperedParameters(a,b,body.a,body.b,radiusA:ra,radiusB:rb).x
        let c=p[0]+(p[1]-p[0])*uv.x,d=p[2]+(p[3]-p[2])*uv.x
        let rc=radius.x+(radius.y-radius.x)*uv.x,rd=radius.z+(radius.w-radius.z)*uv.x
        uv.y=RigCollision.taperedParameters(c,d,body.a,body.b,radiusA:rc,radiusB:rd).x
        if length_squared(uv-old)<1e-10 {break}
      }
      let w=patch.weights(uv),point=point(w)
      let t=RigCollision.segmentParameters(point,point,body.a,body.b).y
      let depth=body.radius+dot(radius,w)-length(point-body.a-(body.b-body.a)*t)
      if depth>bestDepth {bestDepth=depth;bestUV=uv;bestT=t}
    }
    let weights=patch.weights(bestUV)
    return(bestUV,weights,point(weights),bestT,bestDepth)
  }
}

extension SecondaryContact {
  /// Distribute a contact displacement through the same four bilinear weights
  /// that locate the contact. The squared weights are the constraint gradient
  /// terms; a pinned corner always receives exactly zero displacement.
  static func applyPatch(_ positions:inout [V3],patch:SecondaryPatch,rig:SecondaryRig,
    weights:SIMD4<Float>,correction:V3) {
    var denominator:Float=0
    for k in 0..<4 {denominator += rig.nodes[patch.nodes[k]].inverseMass*weights[k]*weights[k]}
    guard denominator>1e-10 else {return}
    for k in 0..<4 {positions[patch.nodes[k]] += correction*(rig.nodes[patch.nodes[k]].inverseMass*weights[k]/denominator)}
  }
  static func patch(_ positions:inout [V3],old:[V3],patch:SecondaryPatch,rig:SecondaryRig,
    before:[RigCapsule],after:[RigCapsule],height:(Float,Float)->Float,friction:Float) {
    var shape=ContactPatchShape(positions,patch:patch,rig:rig)
    for (index,body) in after.enumerated() where shape.overlaps(body) {
      let hit=shape.closest(body,patch:patch)
      guard hit.depth>0 else {continue}
      var previous=V3.zero
      for k in 0..<4 {previous += old[patch.nodes[k]]*hit.weights[k]}
      let axisPoint=body.a+(body.b-body.a)*hit.t,delta=hit.point-axisPoint
      let oldBody=before[index],oldPoint=oldBody.a+(oldBody.b-oldBody.a)*hit.t
      let oldDelta=previous-oldPoint
      let n=length_squared(delta)>1e-12 ? normalize(delta)
        :length_squared(oldDelta)>1e-12 ? normalize(oldDelta):shape.normal(hit.uv)
      let displacement=correction(p:hit.point,old:previous,n:n,depth:hit.depth,
        motion:surfaceMotion(oldBody,body,t:hit.t,n:n),friction:friction)
      applyPatch(&positions,patch:patch,rig:rig,weights:hit.weights,correction:displacement)
      shape=ContactPatchShape(positions,patch:patch,rig:rig)
    }
    // Height fields are sampled at nine interior points. The capsule query
    // above searches continuous u/v coordinates and is not this point grid.
    for v:Float in [0.25,0.5,0.75] {for u:Float in [0.25,0.5,0.75] {
      let w=patch.weights(SIMD2(u,v)),point=shape.point(w)
      let (n,depth)=ground(point,radius:dot(shape.radius,w),height:height)
      guard depth>0 else {continue}
      var previous=V3.zero
      for k in 0..<4 {previous += old[patch.nodes[k]]*w[k]}
      applyPatch(&positions,patch:patch,rig:rig,weights:w,
        correction:correction(p:point,old:previous,n:n,depth:depth,motion:.zero,friction:friction))
      shape=ContactPatchShape(positions,patch:patch,rig:rig)
    }}
  }
  static func patchPenetration(_ positions:[V3],patch:SecondaryPatch,rig:SecondaryRig,
    bodies:[RigCapsule],height:(Float,Float)->Float)->(depth:Float,body:String,pinned:Float,pinnedBody:String) {
    let shape=ContactPatchShape(positions,patch:patch,rig:rig)
    var maximum:Float=0,worst="",pinned:Float=0,worstPinned=""
    func record(_ depth:Float,_ weights:SIMD4<Float>,_ body:String) {
      if depth>maximum {maximum=depth;worst=body}
      var movable:Float=0
      for k in 0..<4 {movable += rig.nodes[patch.nodes[k]].inverseMass*weights[k]*weights[k]}
      if movable<=1e-10 && depth>pinned {pinned=depth;worstPinned=body}
    }
    for body in bodies where shape.overlaps(body) {
      let hit=shape.closest(body,patch:patch);record(hit.depth,hit.weights,body.id)
    }
    for v:Float in [0.25,0.5,0.75] {for u:Float in [0.25,0.5,0.75] {
      let w=patch.weights(SIMD2(u,v)),p=shape.point(w)
      record(height(p.x,p.z)+dot(shape.radius,w)-p.y,w,"ground")
    }}
    return(maximum,worst,pinned,worstPinned)
  }
}
