import Foundation
import simd

public struct PoseTarget:Codable,Equatable,Sendable {
  public var joint:String
  public var point:V3
  public var target:V3
  public var weight:Float
  public init(joint:String,point:V3,target:V3,weight:Float=1) {self.joint=joint;self.point=point;self.target=target;self.weight=weight}
}
public struct PoseFreedom:Codable,Equatable,Sendable {
  public var joint:String
  public var channel:String
  public var minimum:Float
  public var maximum:Float
  public init(joint:String,channel:String,minimum:Float,maximum:Float) {self.joint=joint;self.channel=channel;self.minimum=minimum;self.maximum=maximum}
}
public struct PoseFitResult:Codable,Sendable {
  public var poses:[String:JointPose]
  public var maximumError:Float
  public var initialError:Float
  public var iterations:Int
  public var converged:Bool
}
/// Bounded damped least squares over explicitly selected local pose channels.
/// Targets may be anatomical landmarks, measured reference points or held joints.
/// Unselected channels stay exact. No hidden state or stochastic initial guess.
public enum PoseFitting {
  public static func solve(joints:[PartJoint],poses:[String:JointPose],targets:[PoseTarget],freedoms:[PoseFreedom],iterations:Int=32,tolerance:Float=0.002) throws -> PoseFitResult {
    try PartRig.validate(joints);let ids=Set(joints.map(\.id))
    try CraftError.require((1...32).contains(targets.count) && (1...48).contains(freedoms.count) && (1...80).contains(iterations)
      && tolerance.isFinite && (0.0001...0.1).contains(tolerance),"poseFit","Invalid target/channel budget, iteration count or tolerance")
    try CraftError.require(Set(freedoms.map{$0.joint+"/"+$0.channel}).count==freedoms.count,"poseFit","Duplicate degrees of freedom")
    for t in targets {try CraftError.require(ids.contains(t.joint) && CraftMath.finite(t.point,limit:100) && CraftMath.finite(t.target,limit:100) && t.weight.isFinite && (0.001...100).contains(t.weight),"poseFit.target","Invalid target")}
    for f in freedoms {try CraftError.require(ids.contains(f.joint) && ["x","y","z","pitch","yaw","roll"].contains(f.channel)
      && f.minimum.isFinite && f.maximum.isFinite && f.minimum<f.maximum && abs(f.minimum)<=360 && abs(f.maximum)<=360,
      "poseFit.freedom","Invalid joint/channel bounds")}
    func axis(_ f:PoseFreedom)->Int {switch f.channel {case "x","pitch":return 0;case "y","yaw":return 1;default:return 2}}
    func rotation(_ f:PoseFreedom)->Bool {["pitch","yaw","roll"].contains(f.channel)}
    func value(_ p:[String:JointPose],_ f:PoseFreedom)->Float {let q=p[f.joint] ?? JointPose();return rotation(f) ? q.rotation[axis(f)]:q.offset[axis(f)]}
    func set(_ p:inout [String:JointPose],_ f:PoseFreedom,_ v:Float) {
      var q=p[f.joint] ?? JointPose();let v=max(f.minimum,min(f.maximum,v))
      if rotation(f) {q.rotation[axis(f)]=v} else {q.offset[axis(f)]=v};p[f.joint]=q
    }
    func residual(_ poses:[String:JointPose])->[Float] {
      let m=PartRig.matrices(joints,poses:poses)
      return targets.flatMap{t->[Float] in let d=(CraftMath.point(m[t.joint]!,t.point)-t.target)*sqrt(t.weight);return [d.x,d.y,d.z]}
    }
    func norm(_ r:[Float])->Float {zip(r,r).reduce(0){$0+$1.0*$1.1}}
    func maximum(_ r:[Float])->Float {targets.indices.map{i in length(V3(r[i*3],r[i*3+1],r[i*3+2]))/sqrt(targets[i].weight)}.max() ?? 0}
    var result=poses
    // Bounds are authoritative even if the input seed lies outside them.
    for f in freedoms {set(&result,f,value(result,f))}
    var r=residual(result),used=0
    let initial=maximum(r),n=freedoms.count
    for iteration in 0..<iterations {
      if maximum(r)<=tolerance {break};used=iteration+1
      var jacobian:[[Float]]=[]
      for f in freedoms {
        let x=value(result,f),epsilon:Float=rotation(f) ? 0.02:0.0002
        var a=result,b=result;set(&a,f,x+epsilon);set(&b,f,x-epsilon)
        let delta=value(a,f)-value(b,f),ra=residual(a),rb=residual(b)
        jacobian.append(zip(ra,rb).map{($0.0-$0.1)/max(delta,1e-8)})
      }
      var matrix=Array(repeating:Array(repeating:Float(0),count:n+1),count:n)
      for i in 0..<n {
        for j in 0..<n {matrix[i][j]=zip(jacobian[i],jacobian[j]).reduce(0){$0+$1.0*$1.1}}
        matrix[i][n] = -zip(jacobian[i],r).reduce(0){$0+$1.0*$1.1}
        matrix[i][i] += max(1e-8,matrix[i][i]*0.001)
      }
      // Partial-pivot Gaussian elimination, at most 48 channels.
      for k in 0..<n {
        let pivot=(k..<n).max{abs(matrix[$0][k])<abs(matrix[$1][k])}!
        if pivot != k {matrix.swapAt(k,pivot)}
        let d=matrix[k][k]
        if abs(d)<1e-12 {continue}
        for j in k...n {matrix[k][j]/=d}
        for i in 0..<n where i != k {
          let factor=matrix[i][k];for j in k...n {matrix[i][j]-=factor*matrix[k][j]}
        }
      }
      var accepted=false,factor:Float=1
      for _ in 0..<10 {
        var candidate=result
        for i in 0..<n {set(&candidate,freedoms[i],value(result,freedoms[i])+matrix[i][n]*factor)}
        let next=residual(candidate)
        if norm(next)<norm(r) {result=candidate;r=next;accepted=true;break};factor *= 0.5
      }
      if !accepted {break}
    }
    let error=maximum(r)
    return PoseFitResult(poses:result,maximumError:error,initialError:initial,iterations:used,converged:error<=tolerance)
  }
}
