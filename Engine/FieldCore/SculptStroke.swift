import Foundation
import simd

public enum SculptBrush: String, Codable, Sendable { case grab, inflate, flatten, crease, smooth, scrape, fill, clay, pinch }

public struct SculptProtection:Codable,Equatable,Sendable {
  public var id:String
  public var center:V3
  public var radius:V3
  public var innerFraction:Float?
  public init(id:String,center:V3,radius:V3,innerFraction:Float?=nil) {self.id=id;self.center=center;self.radius=radius;self.innerFraction=innerFraction}
  public func validate() throws {
    let inner=innerFraction ?? 0.7
    try CraftError.require(!id.isEmpty && CraftMath.finite(center,limit:100) && CraftMath.finite(radius,limit:20)
      && radius.min()>=0.005 && inner.isFinite && (0...0.9).contains(inner),"sculpt.protect","Use finite ellipsoids, radii 0.005…20 m and inner fraction 0…0.9")
  }
  public func retention(_ p:V3)->Float {
    // Exact interior protection with an explicitly authored C2 feather.
    let inner=innerFraction ?? 0.7
    return CraftMath.smooth((length((p-center)/radius)-inner)/(1-inner))
  }
}
public struct SculptProfile:Codable,Equatable,Sendable {
  public var tangent:V3
  /// Tangent, bitangent and normal support radii in metres.
  public var radii:V3
  public var hardness:Float
  public var planeOffset:Float
  public var protect:[SculptProtection]
  public init(tangent:V3,radii:V3,hardness:Float=0,planeOffset:Float=0,protect:[SculptProtection]=[]) {
    self.tangent=tangent;self.radii=radii;self.hardness=hardness;self.planeOffset=planeOffset;self.protect=protect
  }
}
public struct SculptLayer:Codable,Equatable,Sendable {
  public var id:String
  public var opacity:Float
  public var enabled:Bool
  public var strokes:[SculptStroke]
  public init(id:String,opacity:Float=1,enabled:Bool=true,strokes:[SculptStroke]=[]) {
    self.id=id;self.opacity=opacity;self.enabled=enabled;self.strokes=strokes
  }
  public var activeStrokes:[SculptStroke] {
    guard enabled && opacity>0 else {return []}
    return strokes.map{var s=$0;s.strength *= opacity;return s}
  }
}

/// A stroke is a spatial field, not a list of disposable vertex indices. Samples
/// describe a polyline in bind metres; closest-segment coverage is independent of
/// resampling density. Mirroring evaluates the reflected field, including vectors.
public struct SculptStroke: Codable, Equatable, Sendable {
  public var id: String
  public var part: String
  public var brush: SculptBrush
  public var points: [V3]
  public var radius: Float
  public var strength: Float
  public var direction: V3
  public var mirrorX: Bool
  public var profile:SculptProfile?
  public init(id:String,part:String,brush:SculptBrush,points:[V3],radius:Float,
    strength:Float,direction:V3 = V3(0,1,0),mirrorX:Bool = false) {
    self.id=id;self.part=part;self.brush=brush;self.points=points;self.radius=radius
    self.strength=strength;self.direction=direction;self.mirrorX=mirrorX
  }
  public func validate() throws {
    try CraftError.require(!id.isEmpty && id.count<=80 && !part.isEmpty && (1...256).contains(points.count), "stroke.identity", id)
    try CraftError.require(points.allSatisfy{CraftMath.finite($0,limit:100)} && CraftMath.finite(direction,limit:10)
      && length(direction)>0.001 && radius.isFinite && (0.005...10).contains(radius)
      && strength.isFinite && abs(strength)<=1 && (brush != .smooth || strength>=0), "stroke.values", id)
    if let p=profile {
      try CraftError.require(CraftMath.finite(p.tangent,limit:10) && length(cross(p.tangent,direction))>0.001
        && CraftMath.finite(p.radii,limit:10) && p.radii.min()>=0.005 && p.hardness.isFinite && (0...0.85).contains(p.hardness)
        && p.planeOffset.isFinite && abs(p.planeOffset)<=0.5 && p.protect.count<=32,
        "stroke.\(id).profile","Use a nonparallel tangent, radii 0.005…10 m, hardness 0…0.85 and plane offset within ±0.5 m")
      for mask in p.protect {try mask.validate()}
    }
  }
  public func nearest(_ p:V3)->V3 {
    var closest=points[0],best=length_squared(p-closest)
    for i in 1..<points.count {
      let a=points[i-1],d=points[i]-a,t=max(0,min(1,dot(p-a,d)/max(1e-12,dot(d,d)))),q=a+d*t
      let error=length_squared(p-q)
      if error<best {best=error;closest=q}
    }
    return closest
  }
  public func coverage(_ p:V3)->Float {
    if let profile {
      let n=normalize(direction),t=normalize(profile.tangent-n*dot(profile.tangent,n)),b=cross(n,t),q=p-nearest(p)
      let local=V3(dot(q,t),dot(q,b),dot(q,n))/profile.radii
      let r=length(local),w=1-CraftMath.smooth((r-profile.hardness)/(1-profile.hardness))
      return profile.protect.reduce(w){$0*$1.retention(p)}
    }
    let q=p-nearest(p),r=length_squared(q)/(radius*radius)
    return pow(max(0,1-r),3)
  }
  public func displacement(_ p:V3)->V3 {
    let n=normalize(direction),q=p-nearest(p),w=coverage(p)
    switch brush {
    case .grab,.inflate: return n*(strength*w)
    case .flatten: return -n*(dot(q,n)*strength*w)
    case .scrape:return -n*max(0,dot(q,n)-(profile?.planeOffset ?? 0))*strength*w
    case .fill:return n*max(0,(profile?.planeOffset ?? 0)-dot(q,n))*strength*w
    case .clay:return n*((profile?.planeOffset ?? 0)-dot(q,n))*strength*w
    case .pinch:return -(q-n*dot(q,n))*(strength*w)
    case .crease:
      let tangent=q-n*dot(q,n)
      return (-n*radius-tangent)*(strength*w)
    case .smooth: return .zero // Topology-aware compiler operation.
    }
  }
  public var reflected: SculptStroke {
    var s=self;s.points=points.map{V3(-$0.x,$0.y,$0.z)};s.direction.x *= -1;s.mirrorX=false
    if var p=s.profile {p.tangent.x *= -1;p.protect=p.protect.map{var m=$0;m.center.x *= -1;return m};s.profile=p}
    return s
  }
  public func field(_ p:V3)->(position:V3,jacobian:simd_float3x3,weight:Float) {
    let mirror=reflected
    func delta(_ q:V3)->V3 {
      guard mirrorX else {return displacement(q)}
      // Smooth partition keeps overlapping mirrored fields continuous at X=0.
      let a=coverage(q),b=mirror.coverage(q),total=a*a+b*b
      if total<1e-20 {return .zero}
      return (displacement(q)*(a*a)+mirror.displacement(q)*(b*b))/total
    }
    var j=matrix_identity_float3x3
    let e=max(0.000001,radius*0.0005)
    for k in 0..<3 {var d=V3.zero;d[k]=e;j[k] += (delta(p+d)-delta(p-d))/(2*e)}
    return (p+delta(p),j,max(coverage(p),mirrorX ? mirror.coverage(p):0))
  }
}

public struct AnatomyLandmark: Codable, Equatable, Sendable {
  public var id:String
  public var part:String
  public var position:V3
  public var note:String
  public init(id:String,part:String,position:V3,note:String="") {
    self.id=id;self.part=part;self.position=position;self.note=note
  }
}

public enum CraftMath {
  public static func finite(_ v:V3,limit:Float)->Bool {(0..<3).allSatisfy{v[$0].isFinite && abs(v[$0])<=limit}}
  public static func point(_ m:simd_float4x4,_ p:V3)->V3 {let q=m*SIMD4(p,1);return V3(q.x,q.y,q.z)}
  public static func smooth(_ t:Float)->Float {let s=min(1,max(0,t));return s*s*s*(10+s*(-15+6*s))}
}
public struct CraftError: LocalizedError {
  public var path:String
  public var reason:String
  public init(path:String,reason:String) {self.path=path;self.reason=reason}
  public var errorDescription:String? {"Craft \(path): \(reason)"}
  public static func require(_ condition:Bool,_ path:String,_ reason:String) throws {
    if !condition {throw CraftError(path:path,reason:reason)}
  }
}
