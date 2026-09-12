import Foundation
import simd

/// Source-domain coverage for unresolved guide-aligned strands. The footprint
/// integrates a periodic pulse, avoiding unfiltered subpixel stripe flicker.
/// This is surface coverage, not volume extinction or multiple scattering.
public enum GroomCoverage {
  public static func random(_ seed: UInt32) -> Float {
    var x=seed &+ 0x9e3779b9
    x=(x^(x>>16)) &* 2246822519;x=(x^(x>>13)) &* 3266489917
    return Float((x^(x>>16))&0xffffff)/Float(0x1000000)
  }
  static func smooth(_ a: Float,_ b: Float,_ x: Float) -> Float {
    let t=max(0,min(1,(x-a)/max(0.000001,b-a)));return t*t*(3-2*t)
  }
  public static func sample(_ coordinates: SIMD4<Float>, footprint: SIMD2<Float>) -> Float {
    let q=coordinates
    if q.z<=0 {return 1}
    let t=max(0,min(1,q.y))
    let phase=q.x+0.12*sin(t*11)*sin(t*Float.pi)
    let span=max(0.002,abs(footprint.x))
    let duty=q.z*(1-0.55*smooth(0.55,1,t))
    func integral(_ x: Float) -> Float {
      floor(x)*duty+max(0,min(duty,x-floor(x)-0.5+duty*0.5))
    }
    let localPhase=phase-floor(phase)
    let pulse=max(0,min(1,(integral(localPhase+span*0.5)-integral(localPhase-span*0.5))/span))
    let variation=max(0.001,q.w),end=1-variation*random(UInt32(max(0,floor(phase))))
    let feather=max(0.008,abs(footprint.y))
    let individual=1-smooth(end-feather,end,t)
    let mean=max(0,min(1,(1-t)/variation))
    let blend=smooth(0.6,1.6,span)
    return max(0,min(1,pulse*(individual*(1-blend)+mean*blend)))
  }
}
