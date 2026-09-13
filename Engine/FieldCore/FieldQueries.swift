import simd

/// Conservative field traversal, shared by gameplay visibility and collision.
public enum FieldQueries {
  public static func visible(from start:V3,to end:V3,solid:Shape)->Bool {
    let delta=end-start,length=simd_length(delta)
    guard length>0.001 else {return true}
    let ray=delta/length,implicit=solid.semantics == .implicit
    var t:Float=0.02
    for _ in 0..<512 {
      if t>=length-0.03 {return true}
      let point=start+ray*t,d=implicit ? solid.conservativeDistance(at:point):solid.value(at:point)
      if d<0.008 {return false}
      t += max(0.008,d*0.9)
    }
    return false
  }
  /// Sweep a vertical capsule through short substeps. Reject an unresolved
  /// penetration rather than teleporting through a thin wall or a low ceiling.
  public static func moveCapsule(eye:V3,delta:V3,solid:Shape,radius:Float=0.28,eyeHeight:Float=1.7)->V3 {
    let count=max(1,Int(ceil(simd_length(delta)/0.10))),implicit=solid.semantics == .implicit
    var result=eye
    for _ in 0..<count {
      var candidate=result+delta/Float(count)
      for _ in 0..<6 {
        for y in [radius-eyeHeight,-eyeHeight*0.5,Float(0)] {
          let q=candidate+V3(0,y,0),sample=solid.sample(at:q)
          let distance=implicit ? solid.conservativeDistance(at:q):sample.value
          if distance<radius,simd_length_squared(sample.gradient)>1e-8 {
            candidate += simd_normalize(sample.gradient)*min(0.20,radius-distance)
          }
        }
      }
      if [radius-eyeHeight,-eyeHeight*0.5,Float(0)].allSatisfy({let p=candidate+V3(0,$0,0);return (implicit ? solid.conservativeDistance(at:p):solid.value(at:p))>=radius-0.015}) {result=candidate}
    }
    return result
  }
}


extension Shape {
  /// Convert general implicit values to a conservative signed distance bound
  /// by certifying empty/interior cubes. This is intentionally a slower query
  /// path; runtime creatures compile dedicated collision representations.
  public func conservativeDistance(at point:V3)->Float {
    guard semantics == .implicit else {return value(at:point)}
    let value=value(at:point)
    if abs(value)<1e-8 {return 0}
    let outside=value>0
    func certified(_ radius:Float)->Bool {
      let r=V3(repeating:radius),i=valueInterval(in:Bounds(point-r,point+r))
      let error:Float=1e-6*(1+abs(i.lower)+abs(i.upper))
      return outside ? i.lower>error:i.upper < -error
    }
    var lo:Float=0,hi:Float=0.001
    for _ in 0..<16 {
      if !certified(hi) {break}
      lo=hi;hi*=2
    }
    for _ in 0..<8 {let middle=(lo+hi)/2;if certified(middle) {lo=middle}else{hi=middle}}
    return outside ? lo:-lo
  }
}
