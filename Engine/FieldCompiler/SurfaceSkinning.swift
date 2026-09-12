import FieldCore
import simd

/// Continuous four-control weights over a rectangular guide grid. Projected
/// decorations use the same parameter field as their host surface, avoiding
/// discontinuous nearest-guide selection across a seam.
public enum SurfaceSkinning {
  public static func weights(uv:SIMD2<Float>,columns:Int,rows:Int)->SkinWeight {
    precondition(columns>=2 && rows>=2 && columns*rows<=64)
    let q=simd_clamp(uv,.zero,SIMD2(repeating:1))*SIMD2(Float(columns-1),Float(rows-1))
    let i=min(columns-2,Int(q.x)),j=min(rows-2,Int(q.y)),u=q.x-Float(i),v=q.y-Float(j)
    let a=UInt32(j*columns+i)
    var result=SkinWeight(a)
    result.joints=SIMD4(a,a+1,a+UInt32(columns),a+UInt32(columns)+1)
    result.weights=SIMD4((1-u)*(1-v),u*(1-v),(1-u)*v,u*v)
    return result
  }
  /// Coarse global seed followed by damped Gauss-Newton projection. A bounded
  /// approximation to closest parameters, not a general self-overlapping UV solver.
  public static func project(_ p:V3,position:(Float,Float)->V3)->SIMD2<Float> {
    var uv=SIMD2<Float>.zero,best=Float.greatestFiniteMagnitude
    for j in 0...8 {for i in 0...8 {
      let q=SIMD2(Float(i)/8,Float(j)/8),d=length_squared(position(q.x,q.y)-p)
      if d<best {best=d;uv=q}
    }}
    for _ in 0..<12 {
      let e:Float=0.001,a=max(0,uv.x-e),b=min(1,uv.x+e),c=max(0,uv.y-e),d=min(1,uv.y+e)
      let du=(position(b,uv.y)-position(a,uv.y))/(b-a)
      let dv=(position(uv.x,d)-position(uv.x,c))/(d-c)
      let r=position(uv.x,uv.y)-p,aa=dot(du,du)+1e-6,bb=dot(du,dv),cc=dot(dv,dv)+1e-6
      let det=aa*cc-bb*bb
      if det<1e-10 {break}
      let x=dot(du,r),y=dot(dv,r),step=SIMD2((cc*x-bb*y)/det,(aa*y-bb*x)/det)
      var accepted=false,factor:Float=1
      for _ in 0..<5 {
        let q=simd_clamp(uv-step*factor,.zero,SIMD2(repeating:1)),error=length_squared(position(q.x,q.y)-p)
        if error<=best {uv=q;best=error;accepted=true;break};factor *= 0.5
      }
      if !accepted || length_squared(step)*factor*factor<1e-10 {break}
    }
    return uv
  }
}
