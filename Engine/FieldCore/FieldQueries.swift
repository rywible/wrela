import simd

/// Conservative field traversal, shared by gameplay visibility and collision.
public enum FieldQueries {
  public static func visible(from start:V3,to end:V3,solid:Shape)->Bool {
    let delta=end-start,length=simd_length(delta)
    guard length>0.001 else {return true}
    let ray=delta/length
    var t:Float=0.02
    for _ in 0..<512 {
      if t>=length-0.03 {return true}
      let d=solid.value(at:start+ray*t)
      if d<0.008 {return false}
      t += max(0.008,d*0.9)
    }
    return false
  }
  /// Sweep a vertical capsule through short substeps. Reject an unresolved
  /// penetration rather than teleporting through a thin wall or a low ceiling.
  public static func moveCapsule(eye:V3,delta:V3,solid:Shape,radius:Float=0.28,eyeHeight:Float=1.7)->V3 {
    let count=max(1,Int(ceil(simd_length(delta)/0.10)))
    var result=eye
    for _ in 0..<count {
      var candidate=result+delta/Float(count)
      for _ in 0..<6 {
        for y in [radius-eyeHeight,-eyeHeight*0.5,Float(0)] {
          let q=candidate+V3(0,y,0),sample=solid.sample(at:q)
          if sample.value<radius,simd_length_squared(sample.gradient)>1e-8 {
            candidate += simd_normalize(sample.gradient)*min(0.20,radius-sample.value)
          }
        }
      }
      if [radius-eyeHeight,-eyeHeight*0.5,Float(0)].allSatisfy({solid.value(at:candidate+V3(0,$0,0))>=radius-0.015}) {result=candidate}
    }
    return result
  }
}
