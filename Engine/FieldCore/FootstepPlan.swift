import Foundation
import simd

public struct AuthoredFootstep: Codable, Equatable, Sendable {
  public var chain:String
  public var lift:Float
  public var land:Float
  public var target:V3
  public var height:Float
  public init(chain:String,lift:Float,land:Float,target:V3,height:Float=0.25) {self.chain=chain;self.lift=lift;self.land=land;self.target=target;self.height=height}
}
public enum FootstepPlan {
  /// Turn a sparse footfall schedule into explicit hold/liftoff/apex/landing
  /// paths. The support-count gate rejects accidentally overlapping swings.
  public static func compile(_ steps:[AuthoredFootstep],initial:[String:V3],duration:Float,minimumSupport:Int) throws -> [ContactPath] {
    try CraftError.require(duration.isFinite && (0.1...60).contains(duration) && (1...32).contains(initial.count)
      && initial.values.allSatisfy{CraftMath.finite($0,limit:20)} && (0...initial.count).contains(minimumSupport) && steps.count<=128,"footsteps","Invalid duration, anchors, support count or step budget")
    for s in steps {
      try CraftError.require(initial[s.chain] != nil && [s.lift,s.land,s.height].allSatisfy(\.isFinite)
        && s.lift>=0 && s.land>s.lift && s.land<=duration && (0.001...2).contains(s.height) && CraftMath.finite(s.target,limit:20),"footstep.\(s.chain)","Invalid swing interval, clearance or target")
    }
    let boundaries=Array(Set([Float(0),duration]+steps.flatMap{[$0.lift,$0.land]})).sorted()
    for i in 1..<boundaries.count {
      let t=(boundaries[i-1]+boundaries[i])*0.5,swinging=steps.filter{$0.lift<=t && $0.land>t}
      try CraftError.require(Set(swinging.map(\.chain)).count==swinging.count,"footsteps","Overlapping swings on one chain at \(t) s")
      try CraftError.require(initial.count-swinging.count>=minimumSupport,"footsteps.support","Only \(initial.count-swinging.count) supports at \(t) s; required \(minimumSupport)")
    }
    return initial.keys.sorted().map{id in
      var position=initial[id]!,keys=[ContactKey(time:0,position:position,planted:true)]
      func append(_ k:ContactKey) {
        if keys.last!.time==k.time {keys[keys.count-1]=k} else {keys.append(k)}
      }
      for step in steps.filter({$0.chain==id}).sorted(by:{$0.lift<$1.lift}) {
        append(ContactKey(time:step.lift,position:position,planted:false))
        var apex=(position+step.target)*0.5;apex.y=max(position.y,step.target.y)+step.height
        append(ContactKey(time:(step.lift+step.land)*0.5,position:apex,planted:false))
        append(ContactKey(time:step.land,position:step.target,planted:true));position=step.target
      }
      if keys.last!.time<duration {append(ContactKey(time:duration,position:position,planted:true))}
      return ContactPath(chain:id,keys:keys,cubic:true)
    }
  }
}
