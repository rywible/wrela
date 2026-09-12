import simd

/// One-way positional contact. Edges are tapered capsules; registered patches
/// are bilinear proxy surfaces. Neither representation is rendered triangles.
/// Contacts are recomputed each iteration; this is discrete contact, not CCD.
enum SecondaryContact {
  static func interpolate(_ a:RigCapsule,_ b:RigCapsule,_ t:Float)->RigCapsule {
    var c=b;c.a=a.a+(b.a-a.a)*t;c.b=a.b+(b.b-a.b)*t;c.radius=a.radius+(b.radius-a.radius)*t;return c
  }
  static func normal(_ delta:V3,oldDelta:V3,edge:V3,body:V3)->V3 {
    if length_squared(delta)>1e-12 {return normalize(delta)}
    if length_squared(oldDelta)>1e-12 {return normalize(oldDelta)}
    let crossAxis=cross(edge,body)
    if length_squared(crossAxis)>1e-12 {return normalize(crossAxis)}
    let axis=length_squared(body)>1e-12 ? normalize(body):V3(1,0,0)
    return normalize(cross(axis,abs(axis.y)<0.9 ? V3(0,1,0):V3(1,0,0)))
  }
  static func surfaceMotion(_ a:RigCapsule,_ b:RigCapsule,t:Float,n:V3)->V3 {
    let oldAxis=a.b-a.a,newAxis=b.b-b.a
    var oldNormal=n
    if length_squared(oldAxis)>1e-10 && length_squared(newAxis)>1e-10 {
      oldNormal=simd_quatf(from:normalize(oldAxis),to:normalize(newAxis)).inverse.act(n)
    }
    let currentCenter:V3=b.a+(b.b-b.a)*t
    let previousCenter:V3=a.a+(a.b-a.a)*t
    let currentSurface:V3=currentCenter+n*b.radius
    let previousSurface:V3=previousCenter+oldNormal*a.radius
    return currentSurface-previousSurface
  }
  /// Coulomb-style positional friction, bounded by the normal correction.
  /// One coefficient supplies the sticking threshold and sliding bound.
  static func correction(p:V3,old:V3,n:V3,depth:Float,motion:V3,friction:Float)->V3 {
    let normalCorrection=n*depth
    let relative=p+normalCorrection-old-motion
    let tangent=relative-n*dot(relative,n),distance=length(tangent)
    let fraction=distance>1e-10 ? min(1,friction*depth/distance):0
    return normalCorrection-tangent*fraction
  }
  static func ground(_ p:V3,radius:Float,height:(Float,Float)->Float)->(V3,Float) {
    let vertical=height(p.x,p.z)+radius-p.y
    guard vertical>0 else {return (V3(0,1,0),0)}
    let n=ContactRig.surfaceNormal(x:p.x,z:p.z,height:height)
    // Retain the height-field model's vertical clearance convention. The local
    // tangent plane supplies response direction; steps are not exact surfaces.
    return (n,vertical*n.y)
  }
  static func overlaps(_ a:V3,_ b:V3,radius:Float,body:RigCapsule)->Bool {
    let padding=V3(repeating:radius+body.radius)
    let low=simd_min(body.a,body.b)-padding,high=simd_max(body.a,body.b)+padding
    let edgeLow=simd_min(a,b),edgeHigh=simd_max(a,b)
    return (0..<3).allSatisfy{edgeHigh[$0]>=low[$0] && edgeLow[$0]<=high[$0]}
  }
  static func particle(_ p:inout V3,old:V3,radius:Float,before:[RigCapsule],after:[RigCapsule],height:(Float,Float)->Float,friction:Float) {
    for (index,b) in after.enumerated() where overlaps(p,p,radius:radius,body:b) {
      let t=RigCollision.segmentParameters(p,p,b.a,b.b).y,q=b.a+(b.b-b.a)*t,delta=p-q
      let depth=b.radius+radius-length(delta)
      if depth>0 {
        let previous=before[index],oldQ=previous.a+(previous.b-previous.a)*t
        let n=normal(delta,oldDelta:old-oldQ,edge:.zero,body:b.b-b.a)
        p += correction(p:p,old:old,n:n,depth:depth,motion:surfaceMotion(previous,b,t:t,n:n),friction:friction)
      }
    }
    let (n,depth)=ground(p,radius:radius,height:height)
    if depth>0 {p += correction(p:p,old:old,n:n,depth:depth,motion:.zero,friction:friction)}
  }
  static func edge(_ positions:inout [V3],old:[V3],a:Int,b:Int,rig:SecondaryRig,before:[RigCapsule],after:[RigCapsule],height:(Float,Float)->Float,friction:Float) {
    let wa=rig.nodes[a].inverseMass,wb=rig.nodes[b].inverseMass
    if wa+wb==0 {return}
    let radiusA=rig.nodes[a].radius,radiusB=rig.nodes[b].radius,radius=max(radiusA,radiusB)
    func apply(_ p:inout [V3],u:Float,n:V3,depth:Float,motion:V3) {
      let v=1-u,weight=wa*v*v+wb*u*u
      if weight<1e-10 {return}
      let point=p[a]*v+p[b]*u,previous=old[a]*v+old[b]*u
      let delta=correction(p:point,old:previous,n:n,depth:depth,motion:motion,friction:friction)/weight
      p[a] += delta*(wa*v);p[b] += delta*(wb*u)
    }
    for (index,body) in after.enumerated() where overlaps(positions[a],positions[b],radius:radius,body:body) {
      let uv=RigCollision.taperedParameters(positions[a],positions[b],body.a,body.b,radiusA:radiusA,radiusB:radiusB)
      let p=positions[a]+(positions[b]-positions[a])*uv.x,q=body.a+(body.b-body.a)*uv.y
      let depth=body.radius+radiusA+(radiusB-radiusA)*uv.x-length(p-q)
      if depth>0 {
        let previous=before[index],oldPoint=old[a]+(old[b]-old[a])*uv.x
        let oldBody=previous.a+(previous.b-previous.a)*uv.y
        let n=normal(p-q,oldDelta:oldPoint-oldBody,edge:positions[b]-positions[a],body:body.b-body.a)
        apply(&positions,u:uv.x,n:n,depth:depth,motion:surfaceMotion(previous,body,t:uv.y,n:n))
      }
    }
    for u:Float in [0.25,0.5,0.75] {
      let point=positions[a]+(positions[b]-positions[a])*u
      let (n,depth)=ground(point,radius:radiusA+(radiusB-radiusA)*u,height:height)
      if depth>0 {apply(&positions,u:u,n:n,depth:depth,motion:.zero)}
    }
  }
  static func penetration(_ p:V3,radius:Float,bodies:[RigCapsule],height:(Float,Float)->Float)->Float {
    var depth=max(0,height(p.x,p.z)+radius-p.y)
    for b in bodies where overlaps(p,p,radius:radius,body:b) {depth=max(depth,b.radius+radius-RigCollision.segmentDistance(p,p,b.a,b.b))}
    return depth
  }
  static func edgePenetration(_ a:V3,_ b:V3,radiusA:Float,radiusB:Float,bodies:[RigCapsule],height:(Float,Float)->Float)->Float {
    var depth:Float=0
    for body in bodies where overlaps(a,b,radius:max(radiusA,radiusB),body:body) {
      let uv=RigCollision.taperedParameters(a,b,body.a,body.b,radiusA:radiusA,radiusB:radiusB)
      depth=max(depth,body.radius+radiusA+(radiusB-radiusA)*uv.x-length(a+(b-a)*uv.x-body.a-(body.b-body.a)*uv.y))
    }
    for i in 0...4 {let u=Float(i)/4,p=a+(b-a)*u;depth=max(depth,height(p.x,p.z)+radiusA+(radiusB-radiusA)*u-p.y)}
    return max(0,depth)
  }
}
