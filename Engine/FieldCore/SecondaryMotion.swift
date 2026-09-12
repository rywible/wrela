import Foundation
import simd

/// Reusable particle/constraint source. Metres, kilograms, seconds; compliance m/N.
public struct SecondaryNode: Codable, Equatable, Sendable {
  public var id: String
  public var joint: String
  public var position: V3
  public var inverseMass: Float
  public var radius: Float
  public var follow: Float
  public var frame:GuideFrameMode = .rotation
  public init(_ id:String, joint:String, position:V3, inverseMass:Float = 40, radius:Float = 0.025, follow:Float = 1) {
    self.id=id;self.joint=joint;self.position=position;self.inverseMass=inverseMass;self.radius=radius;self.follow=follow
  }
  enum CodingKeys:String,CodingKey {case id,joint,position,inverseMass,radius,follow,frame}
  public init(from decoder:Decoder)throws {
    let c=try decoder.container(keyedBy:CodingKeys.self)
    id=try c.decode(String.self,forKey:.id);joint=try c.decode(String.self,forKey:.joint)
    position=try c.decode(V3.self,forKey:.position);inverseMass=try c.decode(Float.self,forKey:.inverseMass)
    radius=try c.decode(Float.self,forKey:.radius);follow=try c.decode(Float.self,forKey:.follow)
    frame=try c.decodeIfPresent(GuideFrameMode.self,forKey:.frame) ?? .rotation
  }

}
public struct SecondaryLink: Codable, Equatable, Sendable {
  public var a: Int
  public var b: Int
  public var compliance: Float
  public var contact: Bool
  public init(_ a:Int,_ b:Int,compliance:Float = 0.000001,contact:Bool = false) {self.a=a;self.b=b;self.compliance=compliance;self.contact=contact}
  enum CodingKeys:String,CodingKey {case a,b,compliance,contact}
  public init(from decoder:Decoder)throws {
    let c=try decoder.container(keyedBy:CodingKeys.self)
    a=try c.decode(Int.self,forKey:.a);b=try c.decode(Int.self,forKey:.b)
    compliance=try c.decode(Float.self,forKey:.compliance)
    contact=try c.decodeIfPresent(Bool.self,forKey:.contact) ?? false
  }
}
/// A bilinear contact surface with corners ordered p00, p10, p01, p11.
/// Its thickness envelope interpolates the four node radii; rendered cloth
/// remains separately audited against this intentionally coarse proxy.
public struct SecondaryPatch: Codable, Equatable, Sendable {
  public var id:String
  public var nodes:SIMD4<Int>
  public init(_ id:String,_ a:Int,_ b:Int,_ c:Int,_ d:Int) {
    self.id=id;nodes=SIMD4(a,b,c,d)
  }
  public func weights(_ uv:SIMD2<Float>)->SIMD4<Float> {
    SIMD4((1-uv.x)*(1-uv.y),uv.x*(1-uv.y),(1-uv.x)*uv.y,uv.x*uv.y)
  }
}
public struct SecondaryRig: Codable, Equatable, Sendable {
  public var nodes:[SecondaryNode]
  public var links:[SecondaryLink]
  public var patches:[SecondaryPatch]
  public init(nodes:[SecondaryNode] = [],links:[SecondaryLink] = [],patches:[SecondaryPatch] = []) {
    self.nodes=nodes;self.links=links;self.patches=patches
  }
  enum CodingKeys:String,CodingKey {case nodes,links,patches}
  public init(from decoder:Decoder)throws {
    let c=try decoder.container(keyedBy:CodingKeys.self)
    nodes=try c.decode([SecondaryNode].self,forKey:.nodes)
    links=try c.decode([SecondaryLink].self,forKey:.links)
    patches=try c.decodeIfPresent([SecondaryPatch].self,forKey:.patches) ?? []
  }
  public func validate() throws {
    guard nodes.count<=512,links.count<=4096,patches.count<=512,
      Set(nodes.map(\.id)).count==nodes.count,Set(patches.map(\.id)).count==patches.count else {throw RigError.invalid}
    for n in nodes {
      guard !n.id.isEmpty,!n.joint.isEmpty,n.id.count<=100,(0..<3).allSatisfy({n.position[$0].isFinite && abs(n.position[$0])<=100}),
        n.inverseMass.isFinite,(0...1000).contains(n.inverseMass),n.radius.isFinite,(0.001...1).contains(n.radius),
        n.follow.isFinite,(0.001...10).contains(n.follow) else {throw RigError.invalid}
    }
    for e in links {
      guard nodes.indices.contains(e.a),nodes.indices.contains(e.b),e.a != e.b,
        length(nodes[e.a].position-nodes[e.b].position)>0.0001,e.compliance.isFinite,(0...10).contains(e.compliance)
      else {throw RigError.invalid}
    }
    for p in patches {
      guard !p.id.isEmpty,p.id.count<=100,Set((0..<4).map{p.nodes[$0]}).count==4,
        (0..<4).allSatisfy({nodes.indices.contains(p.nodes[$0])}) else {throw RigError.invalid}
      let a=nodes[p.nodes[0]].position,b=nodes[p.nodes[1]].position,c=nodes[p.nodes[2]].position,d=nodes[p.nodes[3]].position
      guard length_squared(cross(b-a,c-a))>1e-12,
        length_squared(cross(c-d,b-d))>1e-12 else {throw RigError.invalid}
    }
  }
}
public struct SecondarySettings: Codable, Equatable, Sendable {
  public var enabled = true
  public var gravity:Float = 9.81
  public var damping:Float = 5
  public var followCompliance:Float = 0.6
  public var wind:Float = 0.5
  public var substeps:Int = 4
  public var iterations:Int = 4
  public var edgeContacts = false
  public var friction:Float = 0
  public init() {}
  enum CodingKeys:String,CodingKey {case enabled,gravity,damping,followCompliance,wind,substeps,iterations,edgeContacts,friction}
  public init(from decoder:Decoder)throws {
    let c=try decoder.container(keyedBy:CodingKeys.self)
    enabled=try c.decode(Bool.self,forKey:.enabled);gravity=try c.decode(Float.self,forKey:.gravity)
    damping=try c.decode(Float.self,forKey:.damping);followCompliance=try c.decode(Float.self,forKey:.followCompliance)
    wind=try c.decode(Float.self,forKey:.wind);substeps=try c.decode(Int.self,forKey:.substeps)
    iterations=try c.decode(Int.self,forKey:.iterations)
    edgeContacts=try c.decodeIfPresent(Bool.self,forKey:.edgeContacts) ?? false
    friction=try c.decodeIfPresent(Float.self,forKey:.friction) ?? 0
  }
  public func validate() throws {
    guard gravity.isFinite,(0...20).contains(gravity),damping.isFinite,(0...30).contains(damping),
      followCompliance.isFinite,(0.001...5).contains(followCompliance),wind.isFinite,(0...10).contains(wind),
      (1...8).contains(substeps),(1...12).contains(iterations),friction.isFinite,(0...2).contains(friction) else {throw RigError.invalid}
  }
}
public struct SecondaryState: Codable, Equatable, Sendable {
  public var positions:[V3]
  public var velocities:[V3]
  public init(_ positions:[V3]) {self.positions=positions;velocities=positions.map{_ in .zero}}
}
public struct SecondaryMetrics: Codable, Sendable {
  public var maximumStretch:Float = 0
  public var maximumBendStrain:Float = 0
  public var worstStretch = ""
  public var worstPenetration = ""
  public var worstDisplacement = ""
  public var maximumPenetration:Float = 0
  public var maximumDisplacement:Float = 0
  public var maximumSpeed:Float = 0
  public var maximumEdgePenetration:Float = 0
  public var worstEdgePenetration = ""
  public var maximumPatchPenetration:Float = 0
  public var worstPatchPenetration = ""
  public var pinnedPatchPenetration:Float = 0
  public var worstPinnedPatchPenetration = ""
  public var pinnedPenetration:Float = 0
  public var worstPinnedPenetration = ""
  public init() {}
}

/// Immutable solver coefficients derived from authoritative guide source.
/// Rebuild when the rig or settings change, then share it across replay ticks.
public struct SecondaryProgram: Sendable {
  public let rig:SecondaryRig
  public let settings:SecondarySettings
  fileprivate let dt:Float
  fileprivate let damping:Float
  fileprivate let restLengths:[Float]
  fileprivate let linkAlpha:[Float]
  fileprivate let shapeAlpha:[Float]
  public init(rig:SecondaryRig,settings:SecondarySettings) {
    self.rig=rig;self.settings=settings
    let dt:Float=1/60/Float(settings.substeps)
    self.dt=dt;damping=exp(-settings.damping*dt)
    restLengths=rig.links.map{length(rig.nodes[$0.a].position-rig.nodes[$0.b].position)}
    linkAlpha=rig.links.map{$0.compliance/(dt*dt)}
    shapeAlpha=rig.nodes.map{settings.followCompliance*$0.follow/(dt*dt)}
  }
}
public enum SecondaryMotion {
  /// One exact 60 Hz tick with smaller XPBD steps. Warm multipliers live only
  /// within each substep. Shape attraction is an explicit artistic constraint.
  public static func step(_ state:inout SecondaryState,rig:SecondaryRig,settings:SecondarySettings,
    from:[V3],to:[V3],bodies:[RigCapsule],height:(Float,Float)->Float,time:Float,previousBodies:[RigCapsule]? = nil) {
    step(&state,program:SecondaryProgram(rig:rig,settings:settings),from:from,to:to,bodies:bodies,height:height,time:time,previousBodies:previousBodies)
  }
  public static func step(_ state:inout SecondaryState,program:SecondaryProgram,
    from:[V3],to:[V3],bodies:[RigCapsule],height:(Float,Float)->Float,time:Float,previousBodies:[RigCapsule]? = nil) {
    let rig=program.rig,settings=program.settings
    precondition(state.positions.count==rig.nodes.count && from.count==to.count && to.count==rig.nodes.count)
    let dt=program.dt
    let before=previousBodies ?? bodies
    precondition(before.count==bodies.count && zip(before,bodies).allSatisfy{$0.id==$1.id})
    // Wind is sampled at the exact tick time, not the substep time. Preserve
    // that force while avoiding repeated trigonometry for every substep.
    let accelerationSteps=rig.nodes.indices.map {i -> V3 in
      let wind=V3(sin(time*1.7+Float(i)*0.71),0.15*cos(time*2.1),cos(time*1.1+Float(i)*0.3))*settings.wind
      return (V3(0,-settings.gravity,0)+wind)*(dt*dt)
    }
    var bodyStart=zip(before,bodies).map{SecondaryContact.interpolate($0,$1,0)}
    for sub in 0..<settings.substeps {
      let f=Float(sub+1)/Float(settings.substeps)
      let targets=zip(from,to).map{$0+($1-$0)*f}
      let old=state.positions
      let bodyEnd=zip(before,bodies).map{SecondaryContact.interpolate($0,$1,f)}
      for i in rig.nodes.indices {
        if rig.nodes[i].inverseMass==0 {state.positions[i]=targets[i];continue}
        state.velocities[i] *= program.damping
        state.positions[i] += state.velocities[i]*dt+accelerationSteps[i]
      }
      var lambdas=[Float](repeating:0,count:rig.links.count)
      var shape=[V3](repeating:.zero,count:rig.nodes.count)
      for _ in 0..<settings.iterations {
        for (k,e) in rig.links.enumerated() {
          let delta=state.positions[e.a]-state.positions[e.b],d=length(delta)
          guard d>0.000001 else {continue}
          let rest=program.restLengths[k]
          let wa=rig.nodes[e.a].inverseMass,wb=rig.nodes[e.b].inverseMass,alpha=program.linkAlpha[k]
          guard wa+wb+alpha>0 else {continue}
          let dl = (-(d-rest)-alpha*lambdas[k])/(wa+wb+alpha)
          lambdas[k] += dl
          state.positions[e.a] += delta/d*(wa*dl)
          state.positions[e.b] -= delta/d*(wb*dl)
        }
        for i in rig.nodes.indices {
          let n=rig.nodes[i],w=n.inverseMass
          if w==0 {state.positions[i]=targets[i];continue}
          let alpha=program.shapeAlpha[i]
          let dl = (-(state.positions[i]-targets[i])-alpha*shape[i])/(w+alpha)
          shape[i] += dl;state.positions[i] += w*dl
        }
        if settings.edgeContacts {
          for e in rig.links where e.contact {
            SecondaryContact.edge(&state.positions,old:old,a:e.a,b:e.b,rig:rig,before:bodyStart,after:bodyEnd,height:height,friction:settings.friction)
          }
          for patch in rig.patches {
            SecondaryContact.patch(&state.positions,old:old,patch:patch,rig:rig,before:bodyStart,after:bodyEnd,height:height,friction:settings.friction)
          }
        }
        for i in rig.nodes.indices where rig.nodes[i].inverseMass>0 {
          SecondaryContact.particle(&state.positions[i],old:old[i],radius:rig.nodes[i].radius,before:bodyStart,after:bodyEnd,height:height,friction:settings.friction)
        }
      }
      for i in rig.nodes.indices {state.velocities[i]=(state.positions[i]-old[i])/dt}
      bodyStart=bodyEnd
    }
  }
  public static func metrics(_ state:SecondaryState,rig:SecondaryRig,targets:[V3],bodies:[RigCapsule],height:(Float,Float)->Float)->SecondaryMetrics {
    var m=SecondaryMetrics()
    for e in rig.links {
      let rest=length(rig.nodes[e.a].position-rig.nodes[e.b].position)
      let error=abs(length(state.positions[e.a]-state.positions[e.b])-rest)/rest
      if e.compliance<=0.00001 {
        if error>m.maximumStretch {m.maximumStretch=error;m.worstStretch=rig.nodes[e.a].id+" → "+rig.nodes[e.b].id}
      } else {m.maximumBendStrain=max(m.maximumBendStrain,error)}
      if e.contact {
        let depth=SecondaryContact.edgePenetration(state.positions[e.a],state.positions[e.b],radiusA:rig.nodes[e.a].radius,radiusB:rig.nodes[e.b].radius,bodies:bodies,height:height)
        if depth>m.maximumEdgePenetration {m.maximumEdgePenetration=depth;m.worstEdgePenetration=rig.nodes[e.a].id+" → "+rig.nodes[e.b].id}
      }
    }
    for i in rig.nodes.indices {
      let p=state.positions[i],n=rig.nodes[i]
      let penetration=SecondaryContact.penetration(p,radius:n.radius,bodies:bodies,height:height)
      if n.inverseMass==0 {
        if penetration>m.pinnedPenetration {m.pinnedPenetration=penetration;m.worstPinnedPenetration=n.id}
        continue
      }
      if length(p-targets[i])>m.maximumDisplacement {m.maximumDisplacement=length(p-targets[i]);m.worstDisplacement=n.id}
      m.maximumSpeed=max(m.maximumSpeed,length(state.velocities[i]))
      if penetration>m.maximumPenetration {m.maximumPenetration=penetration;m.worstPenetration=n.id}
    }
    for patch in rig.patches {
      let hit=SecondaryContact.patchPenetration(state.positions,patch:patch,rig:rig,bodies:bodies,height:height)
      if hit.depth>m.maximumPatchPenetration {m.maximumPatchPenetration=hit.depth;m.worstPatchPenetration=patch.id+" → "+hit.body}
      if hit.pinned>m.pinnedPatchPenetration {m.pinnedPatchPenetration=hit.pinned;m.worstPinnedPatchPenetration=patch.id+" → "+hit.pinnedBody}
    }
    return m
  }
}
