import Foundation
import simd

/// Stable, parallel elliptical sections in local metre coordinates. The loft
/// closes with planar end caps; use small end sections or blend caps into other
/// fields when a rounded termination is wanted.
public struct LoftSection:Codable,Equatable,Sendable {
  public var id:String
  public var z:Float
  public var center:SIMD2<Float>
  public var radius:SIMD2<Float>
  public init(id:String,z:Float,center:SIMD2<Float> = .zero,radius:SIMD2<Float>) {
    self.id=id;self.z=z;self.center=center;self.radius=radius
  }
}

/// A continuous authored envelope, not an exact distance or a march bound.
/// Component-wise monotone cubic Hermite interpolation avoids radius overshoot.
public struct SectionLoft:Sendable {
  public struct Segment:Sendable {
    public var z:Float
    public var span:Float
    /// Coefficients in normalized segment t, packed center.x/y, radius.x/y.
    public var a,b,c,d:SIMD4<Float>
  }
  public let sections:[LoftSection]
  public let segments:[Segment]
  public let scale:Float
  public let bounds:Bounds
  public init(_ sections:[LoftSection]) {
    self.sections=sections
    // Invalid authored inputs are rejected by validate before compilation.
    guard sections.count>=2 else {segments=[];scale=1;bounds=Bounds(.zero,.zero);return}
    let values=sections.map{SIMD4($0.center.x,$0.center.y,$0.radius.x,$0.radius.y)}
    let h=zip(sections,sections.dropFirst()).map{max(0.000001,$1.z-$0.z)}
    let secants=zip(values,values.dropFirst()).enumerated().map{($0.element.1-$0.element.0)/h[$0.offset]}
    var slopes=Array(repeating:SIMD4<Float>.zero,count:sections.count)
    slopes[0]=secants[0];slopes[sections.count-1]=secants.last!
    if sections.count>2 {
      for i in 1..<(sections.count-1) {for axis in 0..<4 {
        let l=secants[i-1][axis],r=secants[i][axis]
        if l*r>0 {
          let w1=2*h[i]+h[i-1],w2=h[i]+2*h[i-1]
          slopes[i][axis]=(w1+w2)/(w1/l+w2/r)
        }
      }}
    }
    segments=h.indices.map{i in
      let d=values[i],delta=values[i+1]-d,c=slopes[i]*h[i],end=slopes[i+1]*h[i]
      return Segment(z:sections[i].z,span:h[i],a:c+end-2*delta,b:3*delta-2*c-end,c:c,d:d)
    }
    scale=sections.map{$0.radius.min()}.min()!
    // Monotone interpolation stays in each component's endpoint range.
    let minCenter=sections.map(\.center).reduce(SIMD2<Float>(repeating:.infinity),simd_min)
    let maxCenter=sections.map(\.center).reduce(SIMD2<Float>(repeating:-.infinity),simd_max)
    let maxRadius=sections.map(\.radius).reduce(SIMD2<Float>.zero,simd_max)
    bounds=Bounds(V3(minCenter.x-maxRadius.x,minCenter.y-maxRadius.y,sections[0].z),
                  V3(maxCenter.x+maxRadius.x,maxCenter.y+maxRadius.y,sections.last!.z))
  }
  public func validate() throws {
    try CraftError.require((2...16).contains(sections.count),"loft.sections","Use 2…16 ordered sections")
    try CraftError.require(Set(sections.map(\.id)).count==sections.count,"loft.sections","Section IDs must be unique")
    for (i,s) in sections.enumerated() {
      try CraftError.require(!s.id.isEmpty && s.id.count<=80 && !s.id.contains(".") && s.z.isFinite && abs(s.z)<=20
        && s.center.x.isFinite && s.center.y.isFinite && s.center.max()<=20 && s.center.min() >= -20
        && s.radius.x.isFinite && s.radius.y.isFinite && s.radius.min()>=0.001 && s.radius.max()<=20,
        "loft.section.\(s.id)","Use a stable ID without dots, finite local metre coordinates within ±20 and radii .001…20 m")
      if i>0 {try CraftError.require(s.z-sections[i-1].z>=0.001,"loft.section.\(s.id)","Sections must increase along local Z with at least 1 mm separation")}
    }
  }
  public func profile(at z:Float)->(value:SIMD4<Float>,derivative:SIMD4<Float>) {
    guard let first=segments.first,let last=segments.last else {return (SIMD4(0,0,1,1),.zero)}
    let s=segments.first{z<=$0.z+$0.span} ?? last
    let t=clamp((z-s.z)/s.span,0,1)
    let v=((s.a*t+s.b)*t+s.c)*t+s.d
    let derivative=z<first.z || z>last.z+last.span ? SIMD4<Float>.zero:((3*s.a*t+2*s.b)*t+s.c)/s.span
    return (v,derivative)
  }
  public func sample(at p:V3)->FieldSample {
    let (v,d)=profile(at:p.z),q=SIMD2((p.x-v.x)/v.z,(p.y-v.y)/v.w),r=length(q)
    let localScale=min(v.z,v.w),scaleDerivative=v.z<=v.w ? d.z:d.w
    let radial=(r-1)*localScale,lo=sections.first?.z ?? 0,hi=sections.last?.z ?? 0
    let cap=max(lo-p.z,p.z-hi)
    if cap>=radial {return FieldSample(cap,V3(0,0,lo-p.z>=p.z-hi ? -1:1))}
    guard r>1e-12 else {return FieldSample(radial,V3(0,0,-scaleDerivative))}
    let qz=SIMD2((-d.x-q.x*d.z)/v.z,(-d.y-q.y*d.w)/v.w)
    var gradient=V3(q.x/v.z,q.y/v.w,dot(q,qz))*(localScale/r)
    gradient.z += (r-1)*scaleDerivative
    return FieldSample(radial,gradient)
  }
  public func valueInterval(in region:Bounds)->FieldValueInterval {
    var low=SIMD4<Float>(repeating:.infinity),high = -low
    let a=clamp(region.min.z,sections[0].z,sections.last!.z),b=clamp(region.max.z,sections[0].z,sections.last!.z)
    let points=[profile(at:a).value,profile(at:b).value]+sections.filter{$0.z>a && $0.z<b}.map{SIMD4($0.center.x,$0.center.y,$0.radius.x,$0.radius.y)}
    for p in points {low=simd_min(low,p);high=simd_max(high,p)}
    let lower=SIMD2(region.min.x-high.x,region.min.y-high.y),upper=SIMD2(region.max.x-low.x,region.max.y-low.y)
    let near=simd_max(simd_max(lower,-upper),.zero)/SIMD2(high.z,high.w)
    let far=simd_max(abs(lower),abs(upper))/SIMD2(low.z,low.w)
    let r0=length(near)-1,r1=length(far)-1,s0=min(low.z,low.w),s1=min(high.z,high.w)
    let products=[r0*s0,r0*s1,r1*s0,r1*s1],radialLow=products.min()!,radialHigh=products.max()!
    let lo=sections[0].z,hi=sections.last!.z
    let capLow=max(lo-region.max.z,region.min.z-hi),capHigh=max(lo-region.min.z,region.max.z-hi)
    return FieldValueInterval(max(radialLow,capLow),max(radialHigh,capHigh))
  }
  /// Material coordinates: section interval index + fraction, normalized XY.
  /// Reordering/adding section identities requires explicit rebinding.
  public func coordinate(at p:V3)->V3 {
    let i=segments.firstIndex{p.z<=$0.z+$0.span} ?? max(0,segments.count-1),s=segments[i],v=profile(at:p.z).value
    return V3((p.x-v.x)/v.z,(p.y-v.y)/v.w,Float(i)+(p.z-s.z)/s.span)
  }
  public func point(at q:V3)->V3 {
    let i=min(segments.count-1,max(0,Int(floor(q.z)))),s=segments[i],z=s.z+(q.z-Float(i))*s.span,v=profile(at:z).value
    return V3(v.x+q.x*v.z,v.y+q.y*v.w,z)
  }
}
