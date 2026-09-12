import simd

/// Conservative field-value intervals over an axis-aligned region. The lower
/// bound can reject empty space even when distant smooth CSG branches have made
/// their recursively expanded global bounding box very loose.
public struct FieldValueInterval: Sendable {
  public var lower:Float
  public var upper:Float
  public init(_ lower:Float,_ upper:Float) {self.lower=lower;self.upper=upper}
}

extension Shape {
  public func valueInterval(in region:Bounds)->FieldValueInterval {
    func radial(_ b:Bounds,_ radius:Float)->FieldValueInterval {
      let near=simd_max(simd_max(b.min,-b.max),.zero),far=simd_max(abs(b.min),abs(b.max))
      return FieldValueInterval(length(near)-radius,length(far)-radius)
    }
    func smooth(_ x:Float,_ y:Float,_ k:Float)->Float {
      let h=clamp(0.5+0.5*(y-x)/k,0,1);return y+(x-y)*h-k*h*(1-h)
    }
    switch self {
    case let .sphere(radius):return radial(region,radius)
    case let .translated(a,offset):return a.valueInterval(in:Bounds(region.min-offset,region.max-offset))
    case let .scaled(a,scale):
      let r=a.valueInterval(in:Bounds(region.min/scale,region.max/scale));return FieldValueInterval(r.lower*scale,r.upper*scale)
    case let .stretched(a,scale):
      let r=a.valueInterval(in:Bounds(region.min/scale,region.max/scale)),k=scale.min();return FieldValueInterval(r.lower*k,r.upper*k)
    case let .rotated(a,rotation):
      let inverse=rotation.inverse,m=simd_float3x3(inverse),center=inverse.act((region.min+region.max)/2),half=(region.max-region.min)/2
      let extent=abs(m[0])*half.x+abs(m[1])*half.y+abs(m[2])*half.z
      return a.valueInterval(in:Bounds(center-extent,center+extent).expanded(0.000001*(1+length(center)+length(extent))))
    case let .capsule(a,b,radius):
      let gap=simd_max(simd_max(simd_min(a,b)-region.max,region.min-simd_max(a,b)),.zero)
      let far=simd_max(abs(region.min-a),abs(region.max-a))
      return FieldValueInterval(length(gap)-radius,length(far)-radius)
    case let .union(a,b):
      let x=a.valueInterval(in:region),y=b.valueInterval(in:region);return FieldValueInterval(min(x.lower,y.lower),min(x.upper,y.upper))
    case let .subtract(a,b):
      let x=a.valueInterval(in:region),y=b.valueInterval(in:region);return FieldValueInterval(max(x.lower,-y.upper),max(x.upper,-y.lower))
    case let .smoothSubtract(a,b,k):
      let x=a.valueInterval(in:region),y=b.valueInterval(in:region)
      return FieldValueInterval(-smooth(-x.lower,y.upper,k),-smooth(-x.upper,y.lower,k))
    case let .smoothUnion(a,b,k):
      let x=a.valueInterval(in:region),y=b.valueInterval(in:region)
      return FieldValueInterval(smooth(x.lower,y.lower,k),smooth(x.upper,y.upper,k))
    case .box,.torus:
      let center=(region.min+region.max)/2,radius=length(region.max-region.min)/2,value=value(at:center)
      return FieldValueInterval(value-radius,value+radius)
    }
  }

  /// Every retained leaf encloses all possibly occupied points in that region.
  /// Rejected octants have a strictly positive conservative field interval;
  /// negative interior regions can be accepted whole. This tightens enclosure,
  /// never clips the source or changes its surface definition.
  public func occupiedBounds(depth:Int=7)->Bounds {
    let original=bounds
    guard depth>0 else {return original}
    var result:Bounds?
    func visit(_ region:Bounds,_ remaining:Int,_ field:Shape) {
      let range=field.valueInterval(in:region)
      let allowance:Float=0.000002*(1+abs(range.lower)+abs(range.upper)+length(region.max-region.min))
      if range.lower>allowance {return}
      if remaining==0 || range.upper < -allowance {
        result=result.map{$0.union(region)} ?? region;return
      }
      let regional=remaining>2 ? field.specialized(in:region):field
      let middle=(region.min+region.max)/2
      for z in 0...1 {for y in 0...1 {for x in 0...1 {
        let lo=V3(x==0 ? region.min.x:middle.x,y==0 ? region.min.y:middle.y,z==0 ? region.min.z:middle.z)
        let hi=V3(x==0 ? middle.x:region.max.x,y==0 ? middle.y:region.max.y,z==0 ? middle.z:region.max.z)
        visit(Bounds(lo,hi),remaining-1,regional)
      }}}
    }
    visit(original,min(10,depth),self)
    return result ?? original
  }
}
