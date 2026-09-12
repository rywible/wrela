import Foundation
import simd

/// Source-authored guide chains remove the need to change a species generator
/// when adding a new parting, beard braid, ribbon or secondary appendage.
public struct CraftGuideChain:Codable,Equatable,Sendable {
  public var id:String
  public var joint:String
  public var points:[V3]
  public var pins:Int
  public var inverseMass:Float
  public var radius:Float
  public var compliance:Float
  public init(id:String,joint:String,points:[V3],pins:Int=1,inverseMass:Float=40,radius:Float=0.025,compliance:Float=0.00001) {
    self.id=id;self.joint=joint;self.points=points;self.pins=pins;self.inverseMass=inverseMass;self.radius=radius;self.compliance=compliance
  }
  public var rig:SecondaryRig {
    var rig=SecondaryRig()
    guard (2...16).contains(points.count) else {return rig}
    for (i,p) in points.enumerated() {
      rig.nodes.append(SecondaryNode(id+"-\(i)",joint:joint,position:p,inverseMass:i<pins ? 0:inverseMass,radius:radius))
      if i>0 {rig.links.append(SecondaryLink(i-1,i,compliance:compliance,contact:true))}
      if i>1 {rig.links.append(SecondaryLink(i-2,i,compliance:compliance*10))}
    };return rig
  }
  public func validate(joints:Set<String>) throws {
    try CraftError.require(!id.isEmpty && id.count<=80 && joints.contains(joint) && (2...16).contains(points.count) && (1...points.count).contains(pins),"guideChain.\(id)","Use an existing joint, 2…16 points and at least one pinned root")
    try rig.validate()
  }
}
public struct ClothPanel:Codable,Equatable,Sendable {
  public var id:String
  public var part:String
  public var columns:Int
  public var rows:Int
  public var points:[V3]
  public var attachments:[String]
  public var pins:[Int]
  public var inverseMass:Float
  public var radius:Float
  public var compliance:Float
  public var resolution:Int
  public var color:V3
  public init(id:String,part:String,columns:Int,rows:Int,points:[V3],attachments:[String],pins:[Int],
    inverseMass:Float=20,radius:Float=0.025,compliance:Float=0.00001,resolution:Int=48,color:V3=V3(0.3,0.4,0.36)) {
    self.id=id;self.part=part;self.columns=columns;self.rows=rows;self.points=points;self.attachments=attachments
    self.pins=pins;self.inverseMass=inverseMass;self.radius=radius;self.compliance=compliance;self.resolution=resolution;self.color=color
  }
  public func validate(joints:Set<String>) throws {
    try CraftError.require(!id.isEmpty && id.count<=80 && (2...16).contains(columns) && (2...16).contains(rows) && columns*rows<=64
      && points.count==columns*rows && attachments.count==points.count && attachments.allSatisfy(joints.contains)
      && !pins.isEmpty && Set(pins).count==pins.count && pins.allSatisfy(points.indices.contains)
      && (4...128).contains(resolution) && CraftMath.finite(color,limit:1) && color.min()>=0,"clothPanel.\(id)","Invalid grid, joint attachments, pins, resolution or color")
    try rig.validate()
  }
  public var rig:SecondaryRig {
    var rig=SecondaryRig()
    guard (2...16).contains(columns),(2...16).contains(rows),points.count==columns*rows,attachments.count==points.count else {return rig}
    for j in 0..<rows {for i in 0..<columns {
      let k=j*columns+i
      var node=SecondaryNode(id+"-\(k)",joint:attachments[k],position:points[k],inverseMass:pins.contains(k) ? 0:inverseMass,radius:radius,follow:0.3)
      node.frame = .surface;rig.nodes.append(node)
      if i>0 {rig.links.append(SecondaryLink(k-1,k,compliance:compliance,contact:true))}
      if j>0 {rig.links.append(SecondaryLink(k-columns,k,compliance:compliance,contact:true))}
      if i>0 && j>0 {
        rig.patches.append(SecondaryPatch(id+"-patch-\(i-1)-\(j-1)",k-columns-1,k-columns,k-1,k))
        rig.links.append(SecondaryLink(k-columns-1,k,compliance:compliance*4))
        rig.links.append(SecondaryLink(k-columns,k-1,compliance:compliance*4))
      }
      if i>1 {rig.links.append(SecondaryLink(k-2,k,compliance:compliance*12))}
      if j>1 {rig.links.append(SecondaryLink(k-2*columns,k,compliance:compliance*12))}
    }};return rig
  }
}
extension SecondaryRig {
  public mutating func append(_ other:SecondaryRig) {
    let base=nodes.count;nodes += other.nodes
    links += other.links.map{SecondaryLink($0.a+base,$0.b+base,compliance:$0.compliance,contact:$0.contact)}
    patches += other.patches.map{SecondaryPatch($0.id,$0.nodes[0]+base,$0.nodes[1]+base,$0.nodes[2]+base,$0.nodes[3]+base)}
  }
}
