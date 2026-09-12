import Foundation
import simd

/// Authored collision proxies, independent of render topology. Queries are
/// discrete pose diagnostics, not a rigid-body dynamics or swept CCD solver.
public struct RigCapsule: Codable, Equatable, Sendable {
  public var id: String
  public var joint: String
  public var a: V3
  public var b: V3
  public var radius: Float
  public var ignore: [String]
  public init(_ id:String,joint:String,a:V3,b:V3,radius:Float,ignore:[String]=[]) {
    self.id=id;self.joint=joint;self.a=a;self.b=b;self.radius=radius;self.ignore=ignore
  }
  public func validate() throws {
    guard !id.isEmpty,id.count<=80,!joint.isEmpty,[a,b].allSatisfy({v in (0..<3).allSatisfy{v[$0].isFinite && abs(v[$0])<=100}}),
      radius.isFinite,(0.005...5).contains(radius) else {throw RigError.invalid}
  }
  public func transformed(_ matrix:simd_float4x4)->Self {
    var result=self
    let p=matrix*SIMD4(a,1),q=matrix*SIMD4(b,1)
    result.a=V3(p.x,p.y,p.z);result.b=V3(q.x,q.y,q.z)
    result.radius *= max(length(V3(matrix[0].x,matrix[0].y,matrix[0].z)),max(length(V3(matrix[1].x,matrix[1].y,matrix[1].z)),length(V3(matrix[2].x,matrix[2].y,matrix[2].z))))
    return result
  }
}
public struct RigCollisionHit: Codable, Sendable {
  public var a: String
  public var b: String
  public var separation: Float
}
public enum RigCollision {
  /// Closest parameters on both closed segments, including degenerate points.
  public static func segmentParameters(_ a:V3,_ b:V3,_ c:V3,_ d:V3)->SIMD2<Float> {
    let u=b-a,v=d-c,r=a-c,aa=dot(u,u),bb=dot(v,v),uv=dot(u,v),ur=dot(u,r),vr=dot(v,r)
    var s:Float=0,t:Float=0
    if aa<1e-10 && bb<1e-10 {return .zero}
    if aa<1e-10 {t=min(1,max(0,vr/bb))}
    else if bb<1e-10 {s=min(1,max(0,-ur/aa))}
    else {
      let denominator=aa*bb-uv*uv
      if denominator>1e-10 {s=min(1,max(0,(uv*vr-ur*bb)/denominator))}
      t=(uv*s+vr)/bb
      if t<0 {t=0;s=min(1,max(0,-ur/aa))}
      else if t>1 {t=1;s=min(1,max(0,(uv-ur)/aa))}
    }
    return SIMD2(s,t)
  }
  /// Closest clearance between a linearly tapered guide and a capsule axis.
  /// Distance to a convex segment minus a linear radius is convex in guide u.
  /// Bisection bounds u to 2^-20; equal radii retain the exact segment solution.
  public static func taperedParameters(_ a:V3,_ b:V3,_ c:V3,_ d:V3,radiusA:Float,radiusB:Float)->SIMD2<Float> {
    if abs(radiusA-radiusB)<1e-7 {return segmentParameters(a,b,c,d)}
    let edge=b-a,axis=d-c,axisLength=length_squared(axis),dr=radiusB-radiusA
    func bodyParameter(_ p:V3)->Float {axisLength>1e-12 ? min(1,max(0,dot(p-c,axis)/axisLength)):0}
    if abs(dr)>=length(edge) {
      let u:Float=dr>0 ? 1:0;return SIMD2(u,bodyParameter(a+edge*u))
    }
    var lo:Float=0,hi:Float=1
    for _ in 0..<20 {
      let u=(lo+hi)/2,p=a+edge*u,t=bodyParameter(p),delta=p-(c+axis*t),distance=length(delta)
      if distance<1e-8 {
        // Distance has a cusp on the axis, or a zero-distance plateau when
        // segments are collinear. Its directional derivative is distance to
        // the segment's tangent cone; radius slope chooses the wider end.
        func coneDistance(_ direction:V3)->Float {
          if axisLength<1e-12 {return length(direction)}
          var along=dot(direction,axis)/axisLength
          if t<=0 {along=max(0,along)}
          if t>=1 {along=min(0,along)}
          return length(direction-axis*along)
        }
        if coneDistance(-edge)+dr<0 {hi=u}
        else if coneDistance(edge)-dr<0 {lo=u}
        else {return SIMD2(u,t)}
        continue
      }
      if dot(delta,edge)/distance-dr>0 {hi=u} else {lo=u}
    }
    let u=(lo+hi)/2
    return SIMD2(u,bodyParameter(a+edge*u))
  }
  public static func segmentDistance(_ a:V3,_ b:V3,_ c:V3,_ d:V3)->Float {
    let p=segmentParameters(a,b,c,d)
    return length(a+(b-a)*p.x-c-(d-c)*p.y)
  }
  public static func inspect(_ bodies:[RigCapsule],height:(Float,Float)->Float)->[RigCollisionHit] {
    var result:[RigCollisionHit]=[]
    for (i,a) in bodies.enumerated() {
      var clearance=Float.greatestFiniteMagnitude
      for sample in 0...16 {
        let p=a.a+(a.b-a.a)*Float(sample)/16
        // Vertical height-field sampling is an approximate authoring indicator
        // for these small slopes, not exact capsule-vs-arbitrary-solid contact.
        clearance=min(clearance,p.y-height(p.x,p.z)-a.radius)
      }
      result.append(RigCollisionHit(a:a.id,b:"ground",separation:clearance))
      for b in bodies.dropFirst(i+1) where !a.ignore.contains(b.id) && !b.ignore.contains(a.id) {
        result.append(RigCollisionHit(a:a.id,b:b.id,separation:segmentDistance(a.a,a.b,b.a,b.b)-a.radius-b.radius))
      }
    }
    return result
  }
}
