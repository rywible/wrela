import Foundation
import simd

/// A repeatable authoring fixture. A ramp or a single raised step; no dynamics.
public struct RehearsalSurface: Codable, Equatable, Sendable {
  public var slopeX: Float = 0
  public var slopeZ: Float = 0
  public var stepHeight: Float = 0
  public var stepZ: Float = 0
  public init() {}
  public func validate() throws {
    guard [slopeX,slopeZ,stepHeight,stepZ].allSatisfy(\.isFinite),
      abs(slopeX)<=0.3,abs(slopeZ)<=0.3,(0...0.6).contains(stepHeight),abs(stepZ)<=4
    else {throw RigError.invalid}
  }
  public func height(_ x: Float, _ z: Float) -> Float {
    slopeX*x+slopeZ*z+(z<stepZ ? stepHeight:0)
  }
  public func normal(_ x: Float, _ z: Float) -> V3 {normalize(V3(-slopeX,1,-slopeZ))}
}
public struct ContactChain: Codable, Equatable, Sendable {
  public var id: String
  public var parent: String
  public var upper: String
  public var lower: String
  public var foot: String
  public var pole: V3
  public var sole: Float
  /// Bind-space vector from the terminal joint to its planted contact point.
  /// A digitigrade foot can keep its hock above the floor while its toes plant.
  public var contactOffset: V3?
  public init(id: String,parent: String,upper: String,lower: String,foot: String,pole: V3,sole: Float,contactOffset: V3? = nil) {
    self.id=id;self.parent=parent;self.upper=upper;self.lower=lower;self.foot=foot;self.pole=pole;self.sole=sole;self.contactOffset=contactOffset
  }
  public func validate(joints:[PartJoint]) throws {
    let byID=Dictionary(uniqueKeysWithValues:joints.map{($0.id,$0)})
    try CraftError.require(!id.isEmpty && id.count<=80 && Set([parent,upper,lower,foot]).count==4
      && [parent,upper,lower,foot].allSatisfy{byID[$0] != nil}
      && CraftMath.finite(pole,limit:100) && length(pole)>0.001 && sole.isFinite && (0...2).contains(sole),
      "contactChain.\(id)","Use four distinct existing joints, a nonzero finite pole and sole 0…2 m")
    try CraftError.require(contactOffset.map{CraftMath.finite($0,limit:2)} ?? true,
      "contactChain.\(id).contactOffset","Use a finite bind-space contact vector within ±2 metres")
    try CraftError.require(byID[upper]!.parent==parent && byID[lower]!.parent==upper && byID[foot]!.parent==lower,
      "contactChain.\(id).hierarchy","Expected parent → upper → lower → foot hierarchy")
    try CraftError.require(length(byID[lower]!.pivot-byID[upper]!.pivot)>0.001 && length(byID[foot]!.pivot-byID[lower]!.pivot)>0.001,
      "contactChain.\(id).length","Both segments must exceed 1 mm")
  }
}
public struct ContactTarget: Sendable {
  public var position: V3
  public var planted: Bool
  public init(_ position: V3, planted: Bool) {self.position=position;self.planted=planted}
}
public struct ContactObservation: Codable, Sendable {
  public var id: String
  public var target: V3
  public var actual: V3
  public var knee: V3
  public var normal: V3
  public var planted: Bool
  public var residual: Float
  public var clearance: Float
  public var correction: Float
}
public struct ContactResult {
  public var matrices: [String:simd_float4x4]
  public var contacts: [ContactObservation]
  public init(matrices: [String:simd_float4x4], contacts: [ContactObservation]) {self.matrices=matrices;self.contacts=contacts}
}
public enum ContactRig {
  /// Pull a height field into an upright, uniformly scaled actor frame. World Y
  /// stays up; yaw, translation and positive uniform scale are supported.
  public static func localHeight(frame:simd_float4x4,x:Float,z:Float,height:(Float,Float)->Float)->Float {
    let p=frame*SIMD4<Float>(x,0,z,1)
    return (height(p.x,p.z)-p.y)/max(0.0001,frame.columns.1.y)
  }
  public static func surfaceNormal(x:Float,z:Float,height:(Float,Float)->Float)->V3 {
    let e:Float=0.01
    return normalize(V3(height(x-e,z)-height(x+e,z),2*e,height(x,z-e)-height(x,z+e)))
  }
  public static func solve(joints: [PartJoint], matrices: [String:simd_float4x4],
    chains: [ContactChain], targets: [String:ContactTarget], offsets: [String:V3] = [:],
    height: (Float,Float)->Float = {_,_ in 0}, normal: (Float,Float)->V3 = {_,_ in V3(0,1,0)}, applying: Bool = true) -> ContactResult {
    var output=matrices, observations:[ContactObservation]=[]
    func point(_ m:simd_float4x4,_ p:V3)->V3 {let v=m*SIMD4(p,1);return V3(v.x,v.y,v.z)}
    func translation(_ p:V3)->simd_float4x4 {var m=matrix_identity_float4x4;m.columns.3=SIMD4(p,1);return m}
    func segment(_ a:V3,_ b:V3,_ start:V3,_ end:V3)->simd_float4x4 {
      translation(start)*simd_float4x4(simd_quatf(from:normalize(b-a),to:normalize(end-start)))*translation(-a)
    }
    for chain in chains {
      guard let u=joints.first(where:{$0.id==chain.upper}),let l=joints.first(where:{$0.id==chain.lower}),
        let f=joints.first(where:{$0.id==chain.foot}),let authored=targets[chain.id],
        let parent=matrices[chain.parent],length(l.pivot-u.pivot)>0.001,length(f.pivot-l.pivot)>0.001 else {continue}
      let shoulder=point(parent,u.pivot+u.offset)
      var target=authored.position+(offsets[chain.id] ?? .zero)
      let ground=height(target.x,target.z)
      let n=normalize(normal(target.x,target.z))
      target.y += ground+chain.sole*(1/max(0.1,n.y)-1)
      // Contact fixes sole tilt but preserves the authored paw heading. A turn
      // can rotate a paw during its swing and retain that orientation planted.
      // Project the incoming bind +Z axis onto the ground tangent plane.
      let source=matrices[chain.foot] ?? matrix_identity_float4x4
      let authoredBack=V3(source.columns.2.x,source.columns.2.y,source.columns.2.z)
      var back=authoredBack-n*dot(authoredBack,n)
      if length_squared(back)<1e-8 {
        let fallback:V3=abs(n.z)<0.9 ? V3(0,0,1):V3(1,0,0)
        back=fallback-n*dot(fallback,n)
      }
      back=normalize(back)
      let right=normalize(cross(n,back))
      let orientation=simd_float4x4(SIMD4(right,0),SIMD4(n,0),SIMD4(back,0),SIMD4(0,0,0,1))
      let contactOffset=chain.contactOffset ?? .zero
      let orientedOffset=point(orientation,contactOffset)
      let solution=TwoBoneIK.solve(root:shoulder,target:target-orientedOffset,pole:shoulder+chain.pole,
        upper:length(l.pivot-u.pivot),lower:length(f.pivot-l.pivot))
      let old=point(source,f.pivot+contactOffset)
      if applying {
      output[chain.upper]=segment(u.pivot,l.pivot,shoulder,solution.joint)
      output[chain.lower]=segment(l.pivot,f.pivot,solution.joint,solution.end)
      output[chain.foot]=translation(solution.end)*orientation*translation(-f.pivot)
      }
      let actual=applying ? solution.end+orientedOffset:old
      observations.append(ContactObservation(id:chain.id,target:target,actual:actual,knee:solution.joint,
        normal:n,planted:authored.planted,residual:length(actual-target),
        clearance:(actual.y-height(actual.x,actual.z))*normal(actual.x,actual.z).y-chain.sole,correction:applying ? length(old-actual):0))
    }
    return ContactResult(matrices:output,contacts:observations)
  }
}
