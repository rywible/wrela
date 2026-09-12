import Foundation
import simd

public struct PoseSample: Codable, Equatable, Sendable {
  public var time:Float
  public var poses:[String:JointPose]
  public init(time:Float,poses:[String:JointPose]) {self.time=time;self.poses=poses}
}
public struct ContactKey: Codable, Equatable, Sendable {
  public var time:Float
  public var position:V3
  /// Contact state of the interval beginning at this key.
  public var planted:Bool
  public init(time:Float,position:V3,planted:Bool) {self.time=time;self.position=position;self.planted=planted}
}
public struct ContactPath: Codable, Equatable, Sendable {
  public var chain:String
  public var cubic:Bool?
  public var keys:[ContactKey]
  public init(chain:String,keys:[ContactKey],cubic:Bool?=nil) {self.chain=chain;self.keys=keys;self.cubic=cubic}
  public func supportWeight(_ time:Float,transition:Float)->Float {
    guard sample(time).planted else {return 0}
    guard transition>0 else {return 1}
    var weight:Float=1
    for i in 1..<keys.count where keys[i].planted != keys[i-1].planted {
      let delta=time-keys[i].time
      if keys[i].planted && delta>=0 && delta<transition {weight=min(weight,CraftMath.smooth(delta/transition))}
      if !keys[i].planted && delta<0 && delta > -transition {weight=min(weight,CraftMath.smooth(-delta/transition))}
    };return weight
  }
  public func sample(_ time:Float)->ContactTarget {
    if time<=keys[0].time {return ContactTarget(keys[0].position,planted:keys[0].planted)}
    for i in 1..<keys.count where time<keys[i].time {
      let a=keys[i-1],b=keys[i],u=(time-a.time)/(b.time-a.time)
      if cubic==true && !a.planted {
        func tangent(_ k:Int)->V3 {
          guard k>0,k<keys.count-1,!keys[k].planted,!keys[k-1].planted else {return .zero}
          return (keys[k+1].position-keys[k-1].position)/(keys[k+1].time-keys[k-1].time)
        }
        return ContactTarget(PoseBlending.hermite(a.position,b.position,tangent(i-1),tangent(i),u,b.time-a.time),planted:false)
      }
      let t=CraftMath.smooth(u)
      return ContactTarget(a.position+(b.position-a.position)*t,planted:a.planted)
    }
    return ContactTarget(keys.last!.position,planted:keys.last!.planted)
  }
}
public struct MotionReference: Codable, Equatable, Sendable {
  public var id:String
  public var locator:String
  public var sourceTime:Float
  public var phraseTime:Float
  public var note:String
  public var landmarks:[String:V3]
  public init(id:String,locator:String,sourceTime:Float,phraseTime:Float,note:String,landmarks:[String:V3]=[:]) {
    self.id=id;self.locator=locator;self.sourceTime=sourceTime;self.phraseTime=phraseTime;self.note=note;self.landmarks=landmarks
  }
}
public enum PhraseInterpolation:String,Codable,Sendable {case smooth,linear,spline}
public struct MotionPhrase: Codable, Equatable, Sendable {
  public var id:String
  public var duration:Float
  public var samples:[PoseSample]
  public var interpolation:PhraseInterpolation?
  public var contacts:[ContactPath]
  public var references:[MotionReference]
  public init(id:String,duration:Float,samples:[PoseSample],contacts:[ContactPath]=[],references:[MotionReference]=[],interpolation:PhraseInterpolation?=nil) {
    self.id=id;self.duration=duration;self.samples=samples;self.contacts=contacts;self.references=references;self.interpolation=interpolation
  }
  public func validate(joints:Set<String>,chains:Set<String>) throws {
    let path="phrase.\(id)"
    try CraftError.require(!id.isEmpty && id.count<=80 && duration.isFinite && (0.1...60).contains(duration)
      && (2...241).contains(samples.count),path,"Use 2…241 ordered samples and duration 0.1…60 seconds")
    let ids=Set(samples[0].poses.keys)
    try CraftError.require(!ids.isEmpty && ids.isSubset(of:joints) && samples.first!.time==0 && samples.last!.time==duration,
      path,"Pose samples must span exactly 0…duration and name existing joints")
    var previous:Float = -1
    for sample in samples {
      try CraftError.require(sample.time.isFinite && sample.time>previous && sample.time<=duration && Set(sample.poses.keys)==ids,
        path,"Samples must increase strictly and share the same joint set")
      for (id,p) in sample.poses {
        try CraftError.require(CraftMath.finite(p.offset,limit:20) && CraftMath.finite(p.rotation,limit:360)
          && CraftMath.finite(p.scale,limit:4) && (0..<3).allSatisfy{p.scale[$0]>=0.25},path+"."+id,"Invalid pose transform")
      };previous=sample.time
    }
    try CraftError.require(contacts.count<=32 && Set(contacts.map(\.chain)).count==contacts.count,path,"Duplicate contacts")
    for c in contacts {
      try CraftError.require(chains.contains(c.chain) && (2...241).contains(c.keys.count),path,"Unknown contact or invalid key count")
      try CraftError.require(c.keys.first!.time==0 && c.keys.last!.time==duration,path,"Contact keys must span exactly 0…duration")
      previous = -1
      for (i,k) in c.keys.enumerated() {
        try CraftError.require(k.time.isFinite && k.time>previous && k.time<=duration && CraftMath.finite(k.position,limit:20),path,"Invalid contact key")
        if i>0 && c.keys[i-1].planted {
          try CraftError.require(length(k.position-c.keys[i-1].position)<0.00001,path,"A planted interval cannot move its anchor; insert an unplanted liftoff key")
        };previous=k.time
      }
    }
    try CraftError.require(references.count<=64 && Set(references.map(\.id)).count==references.count,path,"Duplicate references or more than 64 annotations")
    for r in references {
      try CraftError.require(!r.id.isEmpty && r.id.count<=80 && r.locator.count<=2048 && r.note.count<=2000
        && r.sourceTime.isFinite && r.sourceTime>=0 && r.phraseTime.isFinite && (0...duration).contains(r.phraseTime)
        && r.landmarks.count<=64 && r.landmarks.values.allSatisfy{CraftMath.finite($0,limit:100)},path,"Invalid reference annotation")
    }
  }
  public func pose(at time:Float)->[String:JointPose] {
    // Authored controls are exact identities, including protected landing poses.
    // Avoid an unnecessary quaternion round trip at an existing control time.
    if let exact=samples.first(where:{$0.time==time}) {return exact.poses}
    if time<=0 {return samples[0].poses}
    for i in 1..<samples.count where time<samples[i].time {
      let a=samples[i-1],b=samples[i],u=(time-a.time)/(b.time-a.time)
      if interpolation == .spline {
        return a.poses.mapValuesWithKey {id,p in
          func tangent(_ k:Int,_ value:(JointPose)->V3)->V3 {
            guard k>0,k<samples.count-1 else {return .zero}
            return (value(samples[k+1].poses[id]!)-value(samples[k-1].poses[id]!))/(samples[k+1].time-samples[k-1].time)
          }
          func angular(_ k:Int)->V3 {
            guard k>0,k<samples.count-1 else {return .zero}
            let q=PoseBlending.quaternion(samples[k].poses[id]!.rotation)
            let prev=PoseBlending.quaternion(samples[k-1].poses[id]!.rotation),next=PoseBlending.quaternion(samples[k+1].poses[id]!.rotation)
            let before=PoseBlending.log(q.inverse*prev),after=PoseBlending.log(q.inverse*next)
            return (after-before)/(samples[k+1].time-samples[k-1].time)
          }
          let q0=PoseBlending.quaternion(p.rotation),q3=PoseBlending.quaternion(b.poses[id]!.rotation),dt=b.time-a.time
          let q1=q0*PoseBlending.exp(angular(i-1)*(dt/3)),q2=q3*PoseBlending.exp(-angular(i)*(dt/3))
          let q=simd_slerp(simd_slerp(simd_slerp(q0,q1,u),simd_slerp(q1,q2,u),u),simd_slerp(simd_slerp(q1,q2,u),simd_slerp(q2,q3,u),u),u)
          return JointPose(offset:PoseBlending.hermite(p.offset,b.poses[id]!.offset,tangent(i-1,{$0.offset}),tangent(i,{$0.offset}),u,dt),
            rotation:PoseBlending.euler(q),scale:simd_clamp(PoseBlending.hermite(p.scale,b.poses[id]!.scale,tangent(i-1,{$0.scale}),tangent(i,{$0.scale}),u,dt),V3(repeating:0.25),V3(repeating:4)))
        }
      }
      let t=interpolation == .linear ? u:CraftMath.smooth(u)
      return a.poses.mapValuesWithKey {id,p in PoseBlending.blend(p,b.poses[id]!,t)}
    }
    return samples.last!.poses
  }
  public func retimed(to duration:Float)->MotionPhrase {
    var result=self;let ratio=duration/self.duration;result.duration=duration
    result.samples=samples.map{PoseSample(time:$0.time*ratio,poses:$0.poses)}
    result.samples[result.samples.count-1].time=duration
    result.contacts=contacts.map{c in var c=c;c.keys=c.keys.map{var k=$0;k.time *= ratio;return k};c.keys[c.keys.count-1].time=duration;return c}
    result.references=references.map{var r=$0;r.phraseTime *= ratio;return r};return result
  }
  /// Explicit mapping avoids assuming human names, sidedness or anatomy. Reflection
  /// is across bind X=0. Axial rotations transform with determinant(reflection).
  public func mirrored(id:String,jointMap:[String:String],chainMap:[String:String]) throws -> MotionPhrase {
    var result=self;result.id=id
    try CraftError.require(Set(samples[0].poses.keys.map{jointMap[$0] ?? $0}).count==samples[0].poses.count,
      "mirror.jointMap","Mappings must be injective")
    try CraftError.require(Set(contacts.map{chainMap[$0.chain] ?? $0.chain}).count==contacts.count,"mirror.chainMap","Mappings must be injective")
    result.samples=samples.map{s in PoseSample(time:s.time,poses:Dictionary(uniqueKeysWithValues:s.poses.map{id,p in
      var p=p;p.offset.x *= -1;p.rotation.y *= -1;p.rotation.z *= -1;return (jointMap[id] ?? id,p)
    }))}
    result.contacts=contacts.map{c in var c=c;c.chain=chainMap[c.chain] ?? c.chain;c.keys=c.keys.map{var k=$0;k.position.x *= -1;return k};return c}
    result.references=try references.map { reference in
      var r=reference
      try CraftError.require(Set(r.landmarks.keys.map{jointMap[$0] ?? $0}).count==r.landmarks.count,
        "mirror.referenceMap","Reference landmark mappings must be injective")
      r.landmarks=Dictionary(uniqueKeysWithValues:r.landmarks.map{id,p in (jointMap[id] ?? id,V3(-p.x,p.y,p.z))})
      return r
    };return result
  }
}

public struct PhraseLayer: Codable, Equatable, Sendable {
  public var id:String
  public var phrase:String
  public var start:Float
  public var end:Float
  public var sourceStart:Float
  public var sourceEnd:Float
  public var fadeIn:Float
  public var fadeOut:Float
  public var weight:Float
  public var additive:Bool
  public init(id:String,phrase:String,start:Float,end:Float,sourceStart:Float,sourceEnd:Float,
    fadeIn:Float=0,fadeOut:Float=0,weight:Float=1,additive:Bool=false) {
    self.id=id;self.phrase=phrase;self.start=start;self.end=end;self.sourceStart=sourceStart;self.sourceEnd=sourceEnd
    self.fadeIn=fadeIn;self.fadeOut=fadeOut;self.weight=weight;self.additive=additive
  }
  public func sample(_ time:Float)->(time:Float,weight:Float) {
    guard time>=start && time<=end else {return(0,0)}
    let a=fadeIn>0 ? CraftMath.smooth((time-start)/fadeIn):1
    let b=fadeOut>0 ? CraftMath.smooth((end-time)/fadeOut):1
    return(sourceStart+(sourceEnd-sourceStart)*(time-start)/(end-start),weight*a*b)
  }
}
public struct MotionArrangement: Codable, Equatable, Sendable {
  public var clip:String
  public var duration:Float
  public var looping:Bool
  public var layers:[PhraseLayer]
  public init(clip:String,duration:Float,looping:Bool=true,layers:[PhraseLayer]) {
    self.clip=clip;self.duration=duration;self.looping=looping;self.layers=layers
  }
  public func validate(phrases:[MotionPhrase],clips:Set<String>) throws {
    try CraftError.require(clips.contains(clip) && duration.isFinite && (0.1...60).contains(duration)
      && layers.count<=64 && Set(layers.map(\.id)).count==layers.count,"arrangement","Invalid clip/duration or duplicate layers")
    for l in layers {
      guard let p=phrases.first(where:{$0.id==l.phrase}) else {throw CraftError(path:"arrangement.\(l.id)",reason:"Unknown phrase")}
      try CraftError.require(!l.id.isEmpty && [l.start,l.end,l.sourceStart,l.sourceEnd,l.fadeIn,l.fadeOut,l.weight].allSatisfy(\.isFinite)
        && l.start>=0 && l.end>l.start && l.end<=duration && l.sourceStart>=0 && l.sourceEnd>l.sourceStart && l.sourceEnd<=p.duration
        && l.fadeIn>=0 && l.fadeOut>=0 && l.fadeIn+l.fadeOut<=l.end-l.start && (0...1).contains(l.weight),"arrangement.\(l.id)","Invalid layer interval, source range or fade")
      try CraftError.require(!l.additive || p.contacts.isEmpty,"arrangement.\(l.id)","Contact paths need a replacement layer; additive contact offsets are ambiguous")
    }
  }
  public func evaluate(time:Float,phrases:[MotionPhrase],poses:[String:JointPose],contacts:[String:ContactTarget]) -> (poses:[String:JointPose],contacts:[String:ContactTarget]) {
    let time=looping ? max(0,time).truncatingRemainder(dividingBy:duration):max(0,min(duration,time))
    var output=poses,targets=contacts
    for layer in layers {
      let s=layer.sample(time)
      guard s.weight>0,let phrase=phrases.first(where:{$0.id==layer.phrase}) else {continue}
      for (id,p) in phrase.pose(at:s.time) {
        let old=output[id] ?? JointPose()
        output[id]=layer.additive ? PoseBlending.add(old,p,s.weight):PoseBlending.blend(old,p,s.weight)
      }
      for path in phrase.contacts {
        let p=path.sample(s.time),old=targets[path.chain] ?? p
        // A blended moving anchor is never reported as planted. The audit reports
        // such transition intervals explicitly instead of hiding planted slip.
        let same=length_squared(old.position-p.position)<1e-10
        targets[path.chain]=ContactTarget(old.position+(p.position-old.position)*s.weight,
          planted:s.weight>=0.99999 ? p.planted:(old.planted && p.planted && same))
      }
    }
    return(output,targets)
  }
}
public enum PoseBlending {
  public static func hermite(_ a:V3,_ b:V3,_ da:V3,_ db:V3,_ t:Float,_ dt:Float)->V3 {
    let t2=t*t,t3=t2*t
    let left=a*(2*t3-3*t2+1)+da*((t3-2*t2+t)*dt)
    let right=b*(-2*t3+3*t2)+db*((t3-t2)*dt)
    return left+right
  }
  public static func log(_ quaternion:simd_quatf)->V3 {
    let q=quaternion.real<0 ? simd_quatf(vector:-quaternion.vector):quaternion
    let n=length(q.imag)
    return n<1e-8 ? q.imag:q.imag*(atan2(n,q.real)/n)
  }
  public static func exp(_ v:V3)->simd_quatf {
    let n=length(v)
    return n<1e-8 ? simd_quatf(vector:SIMD4(v,1)).normalized:simd_quatf(angle:2*n,axis:v/n)
  }
  public static func quaternion(_ r:V3)->simd_quatf {
    let r=r*(Float.pi/180)
    return simd_quatf(angle:r.z,axis:V3(0,0,1))*simd_quatf(angle:r.y,axis:V3(0,1,0))*simd_quatf(angle:r.x,axis:V3(1,0,0))
  }
  public static func euler(_ q:simd_quatf)->V3 {
    let v=q.normalized.vector,x=v.x,y=v.y,z=v.z,w=v.w
    return V3(atan2(2*(w*x+y*z),1-2*(x*x+y*y)),asin(max(-1,min(1,2*(w*y-z*x)))),atan2(2*(w*z+x*y),1-2*(y*y+z*z)))*(180/Float.pi)
  }
  public static func blend(_ a:JointPose,_ b:JointPose,_ t:Float)->JointPose {
    JointPose(offset:a.offset+(b.offset-a.offset)*t,rotation:euler(simd_slerp(quaternion(a.rotation),quaternion(b.rotation),t)),scale:a.scale+(b.scale-a.scale)*t)
  }
  public static func add(_ a:JointPose,_ b:JointPose,_ t:Float)->JointPose {
    JointPose(offset:a.offset+b.offset*t,rotation:euler(quaternion(a.rotation)*simd_slerp(simd_quatf(angle:0,axis:V3(0,1,0)),quaternion(b.rotation),t)),scale:a.scale*(V3(repeating:1)+(b.scale-V3(repeating:1))*t))
  }
}
extension Dictionary {
  fileprivate func mapValuesWithKey<T>(_ f:(Key,Value)->T)->[Key:T] {Dictionary<Key,T>(uniqueKeysWithValues:map{($0.key,f($0.key,$0.value))})}
}
