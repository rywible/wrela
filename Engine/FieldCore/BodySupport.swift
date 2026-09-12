import Foundation
import simd

public struct BalanceControl: Codable, Equatable, Sendable {
  public var joint:String
  public var strength:Float
  public var maximumShift:Float
  public var transitionSeconds:Float?
  public init(joint:String,strength:Float=1,maximumShift:Float=0.2,transitionSeconds:Float?=nil) {self.joint=joint;self.strength=strength;self.maximumShift=maximumShift;self.transitionSeconds=transitionSeconds}
}
public struct SupportObservation: Codable, Sendable {
  public var centerOfMass:V3
  public var totalMass:Float
  public var supportPolygon:[SIMD2<Float>]
  public var outsideDistance:Float
  public var supported:Bool
  public var nearestSupport:SIMD2<Float>
}
public enum BodySupport {
  public static func inspect(masses:[BodyMass],matrices:[String:simd_float4x4],anchors:[V3])->SupportObservation {
    let mass=masses.reduce(Float(0)){$0+$1.kilograms}
    let center=masses.reduce(V3.zero){$0+CraftMath.point(matrices[$1.joint] ?? matrix_identity_float4x4,$1.center)*$1.kilograms}/max(0.000001,mass)
    let polygon=hull(anchors.map{SIMD2($0.x,$0.z)}),q=SIMD2(center.x,center.z)
    var nearest=q,distance:Float=0,supported=false
    if let first=polygon.first {
      nearest=first;distance=length(q-first)
      if polygon.count>=2 {
        var inside=polygon.count>=3
        for i in polygon.indices {
          let a=polygon[i],b=polygon[(i+1)%polygon.count],d=b-a
          if cross2(d,q-a) < -1e-6 {inside=false}
          let t=max(0,min(1,dot(q-a,d)/max(1e-12,dot(d,d)))),p=a+d*t,e=length(q-p)
          if e<distance {distance=e;nearest=p}
        }
        if inside {nearest=q;distance=0;supported=true}
      }
      if distance<=0.00001 {supported=true}
    }
    return SupportObservation(centerOfMass:center,totalMass:mass,supportPolygon:polygon,
      outsideDistance:distance,supported:supported && mass>0,nearestSupport:nearest)
  }
  public static func hull(_ points:[SIMD2<Float>])->[SIMD2<Float>] {
    let sorted=points.sorted{$0.x==$1.x ? $0.y<$1.y:$0.x<$1.x}
    var unique:[SIMD2<Float>]=[]
    for p in sorted where unique.last.map({length_squared(p-$0)>1e-12}) ?? true {unique.append(p)}
    guard unique.count>2 else {return unique}
    func half(_ ps:[SIMD2<Float>])->[SIMD2<Float>] {
      var out:[SIMD2<Float>]=[]
      for p in ps {
        while out.count>=2 && cross2(out.last!-out[out.count-2],p-out.last!)<=0 {out.removeLast()}
        out.append(p)
      };return out
    }
    return Array(half(unique).dropLast())+Array(half(unique.reversed()).dropLast())
  }
  static func cross2(_ a:SIMD2<Float>,_ b:SIMD2<Float>)->Float {a.x*b.y-a.y*b.x}

  /// Bounded static assistance. It moves the selected body's local horizontal
  /// offset using the measured COM Jacobian; contact IK follows afterward. It
  /// never claims force equilibrium, solves airborne motion or changes footfalls.
  public static func assist(_ control:BalanceControl,joints:[PartJoint],poses:[String:JointPose],masses:[BodyMass],targets:[String:ContactTarget],supportWeights:[String:Float]=[:])->[String:JointPose] {
    let active=targets.keys.sorted().filter{targets[$0]!.planted}
    let weights=Dictionary(uniqueKeysWithValues:active.map{($0,max(0,min(1,supportWeights[$0] ?? 1)))})
    let totalWeight=active.reduce(Float(0)){$0+weights[$1]!}
    guard totalWeight>1e-6,!masses.isEmpty else {return poses}
    // Blend nearest points on nested support hulls. With one transferring foot
    // this is exactly a smooth blend between its planted and lifted polygons.
    // Sorting capacities avoids exponential subset enumeration and remains
    // continuous when simultaneous ramps cross or an active foot disappears.
    let initial=inspect(masses:masses,matrices:PartRig.matrices(joints,poses:poses),anchors:[])
    let levels=Set(weights.values.filter{$0>0}).sorted()
    var previous:Float=0,target=SIMD2<Float>.zero
    for level in levels {
      let anchors=active.filter{weights[$0]!>=level}.map{targets[$0]!.position}
      let projection=inspect(masses:[BodyMass(joint:"support",center:initial.centerOfMass,kilograms:1)],matrices:[:],anchors:anchors).nearestSupport
      target+=projection*(level-previous);previous=level
    }
    let original=SIMD2(initial.centerOfMass.x,initial.centerOfMass.z)
    target=original+(target/previous-original)*control.strength
    var result=poses,total=V3.zero
    for _ in 0..<4 {
      let center=inspect(masses:masses,matrices:PartRig.matrices(joints,poses:result),anchors:[]).centerOfMass
      let desired=target-SIMD2(center.x,center.z)
      if length_squared(desired)<1e-12 {break}
      var columns:[SIMD2<Float>]=[]
      for axis in [0,2] {
        var candidate=result,p=candidate[control.joint] ?? JointPose();p.offset[axis]+=0.001;candidate[control.joint]=p
        let next=inspect(masses:masses,matrices:PartRig.matrices(joints,poses:candidate),anchors:[]).centerOfMass
        columns.append(SIMD2(next.x-center.x,next.z-center.z)/0.001)
      }
      let matrix=simd_float2x2(columns:(columns[0],columns[1]))
      if abs(simd_determinant(matrix))<1e-6 {break}
      let step=matrix.inverse*desired
      var candidate=total+V3(step.x,0,step.y)
      if length(candidate)>control.maximumShift {candidate=normalize(candidate)*control.maximumShift}
      var p=result[control.joint] ?? JointPose();p.offset+=candidate-total;result[control.joint]=p
      if length_squared(candidate-total)<1e-12 {break};total=candidate
    };return result
  }
}
