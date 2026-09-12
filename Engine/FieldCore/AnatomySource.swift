import Foundation
import simd

public enum AnatomyPrimitive: String, Codable, Sendable { case ellipsoid, capsule }
public enum AnatomyOperation: String, Codable, Sendable { case add, cut }
public enum AnatomyRole: String, Codable, Sendable { case surface, internalStructure }

/// A named volume is the durable source of form, region identity and binding.
/// Euler rotation is right-handed X then Y then Z, in degrees. Capsules use a
/// circular cross-section and world-space endpoints; ellipsoids can be oblique.
public struct AnatomyElement: Codable, Equatable, Sendable {
  public var id:String
  public var region:String
  public var primitive:AnatomyPrimitive
  public var operation:AnatomyOperation
  public var role:AnatomyRole
  public var center:V3
  public var end:V3
  public var radius:V3
  public var rotation:V3
  public var blend:Float
  /// Optional explicit rounding for subtraction. Legacy `blend` only affects
  /// additions; decoding old cuts retains their exact prior surface.
  public var cutBlend:Float?
  public var color:V3
  public var jointWeights:[String:Float]
  public init(id:String,region:String,primitive:AnatomyPrimitive = .ellipsoid,
    operation:AnatomyOperation = .add,role:AnatomyRole = .surface,center:V3,radius:V3,
    end:V3 = .zero,rotation:V3 = .zero,blend:Float = 0.02,
    color:V3 = V3(repeating:0.6),jointWeights:[String:Float] = [:],cutBlend:Float?=nil) {
    self.id=id;self.region=region;self.primitive=primitive;self.operation=operation
    self.role=role;self.center=center;self.end=end;self.radius=radius
    self.rotation=rotation;self.blend=blend;self.color=color;self.jointWeights=jointWeights
    self.cutBlend=cutBlend
  }
  public var orientation:simd_quatf {
    let r=rotation*(Float.pi/180)
    return simd_normalize(simd_quatf(angle:r.z,axis:V3(0,0,1))*simd_quatf(angle:r.y,axis:V3(0,1,0))*simd_quatf(angle:r.x,axis:V3(1,0,0)))
  }
  public var shape:Shape {
    switch primitive {
    case .ellipsoid:return Shape.sphere(1).stretched(radius).rotated(orientation).moved(center)
    case .capsule:return .capsule(center,end,radius.x)
    }
  }
  public func validate(joints:Set<String>? = nil) throws {
    let path="anatomy.element.\(id)"
    try CraftError.require(!id.isEmpty && id.count<=80 && !region.isEmpty && region.count<=80,path,"Use nonempty stable IDs and semantic regions of at most 80 characters")
    try CraftError.require(CraftMath.finite(center,limit:100) && CraftMath.finite(end,limit:100)
      && CraftMath.finite(radius,limit:20) && radius.min()>=0.001 && CraftMath.finite(rotation,limit:360)
      && blend.isFinite && (0...2).contains(blend),path,"Use finite metre dimensions, radii 0.001…20 m, blend 0…2 m and rotations within ±360 degrees")
    try CraftError.require(CraftMath.finite(color,limit:1) && color.min()>=0,path,"Color must be linear RGB in 0…1")
    if let cutBlend {try CraftError.require(operation == .cut && cutBlend.isFinite && (0...2).contains(cutBlend),path+".cutBlend","Cut rounding is 0…2 field metres and applies only to subtraction")}
    try CraftError.require(jointWeights.count<=4 && jointWeights.keys.allSatisfy{!$0.isEmpty && $0.count<=80 && (joints?.contains($0) ?? true)}
      && jointWeights.values.allSatisfy{$0.isFinite && $0>=0}
      && (jointWeights.isEmpty || abs(jointWeights.values.reduce(0,+)-1)<0.0001),path,"Use at most four existing joints and normalized nonnegative weights")
    if primitive == .capsule {
      try CraftError.require(length(end-center)>=0.001 && abs(radius.x-radius.y)<0.000001 && abs(radius.x-radius.z)<0.000001
        && rotation == .zero,path,"Capsules use distinct world-space endpoints, equal radii and zero rotation; orient with the endpoints")
    }
    try CraftError.require(role != .internalStructure || operation == .add,path,"Internal structures must be additions; they do not alter the exterior skin")
  }
  private var capsuleFrame:(V3,V3,V3) {
    let axis=normalize(end-center),reference=abs(axis.y)<0.9 ? V3(0,1,0):V3(1,0,0)
    let normal=normalize(cross(axis,reference))
    return (axis,normal,cross(axis,normal))
  }
  /// Source coordinates survive tessellation changes. These are material
  /// coordinates in the primitive, independent of transient triangle indices.
  public func coordinate(at point:V3)->V3 {
    if primitive == .ellipsoid {return orientation.inverse.act(point-center)/radius}
    let (axis,normal,binormal)=capsuleFrame,p=point-center
    return V3(dot(p,axis)/length(end-center),dot(p,normal)/radius.x,dot(p,binormal)/radius.x)
  }
  public func point(at coordinate:V3)->V3 {
    if primitive == .ellipsoid {return center+orientation.act(coordinate*radius)}
    let (axis,normal,binormal)=capsuleFrame
    return center+axis*(coordinate.x*length(end-center))+(normal*coordinate.y+binormal*coordinate.z)*radius.x
  }
}

public struct AnatomySample: Sendable {
  public var field:FieldSample
  public var element:String
  public var region:String
  public var color:V3
  public var jointWeights:[String:Float]
}

/// Ordered positive/negative volumes form the actual authoring surface. Internal
/// volumes remain independently queryable mathematical source for collision,
/// rig placement and exposed anatomical studies; they do not inflate the skin.
public struct AnatomySource: Codable, Equatable, Sendable {
  public var id:String
  public var part:String
  public var elements:[AnatomyElement]
  public var resolution:Int
  public var material:Int
  public var roughness:Float
  public var metallic:Float
  /// Legacy output meshes subsumed by this continuous surface. Their authored
  /// joints remain available to the rig and skin palette.
  public var replaces:[String]
  /// Skin influence transition width in metres, independent of the geometric
  /// blend radius. Zero retains the geometric CSG partition exactly.
  public var skinBlend:Float
  public init(id:String,part:String,elements:[AnatomyElement],resolution:Int=64,
    material:Int=7,roughness:Float=0.65,metallic:Float=0,replaces:[String]=[],skinBlend:Float=0) {
    self.id=id;self.part=part;self.elements=elements;self.resolution=resolution
    self.material=material;self.roughness=roughness;self.metallic=metallic
    self.replaces=replaces
    self.skinBlend=skinBlend
  }
  private enum CodingKeys:String,CodingKey {case id,part,elements,resolution,material,roughness,metallic,replaces,skinBlend}
  public init(from decoder:Decoder) throws {
    let c=try decoder.container(keyedBy:CodingKeys.self)
    id=try c.decode(String.self,forKey:.id);part=try c.decode(String.self,forKey:.part)
    elements=try c.decode([AnatomyElement].self,forKey:.elements)
    resolution=try c.decode(Int.self,forKey:.resolution);material=try c.decode(Int.self,forKey:.material)
    roughness=try c.decode(Float.self,forKey:.roughness);metallic=try c.decode(Float.self,forKey:.metallic)
    replaces=try c.decodeIfPresent([String].self,forKey:.replaces) ?? []
    skinBlend=try c.decodeIfPresent(Float.self,forKey:.skinBlend) ?? 0
  }
  public var surfaceElements:[AnatomyElement] {elements.filter{$0.role == .surface}}
  public var internalElements:[AnatomyElement] {elements.filter{$0.role == .internalStructure}}
  public func validate(joints:Set<String>? = nil) throws {
    let path="anatomy.\(id)"
    try CraftError.require(!id.isEmpty && id.count<=80 && !part.isEmpty && part.count<=80,path,"Use nonempty source and part IDs of at most 80 characters")
    try CraftError.require(replaces.count<=32 && Set(replaces).count==replaces.count
      && replaces.allSatisfy{!$0.isEmpty && $0.count<=80 && $0 != part},path+".replaces","Use at most 32 unique output part names, excluding this source's own part")
    try CraftError.require(skinBlend.isFinite && (0...1).contains(skinBlend),path+".skinBlend","Use a skin influence transition of 0…1 metre")
    try CraftError.require((1...128).contains(elements.count) && Set(elements.map(\.id)).count==elements.count,path,"Use 1…128 uniquely named anatomical elements")
    try CraftError.require((24...128).contains(resolution) && (0...9).contains(material)
      && roughness.isFinite && (0.05...1).contains(roughness) && metallic.isFinite && (0...1).contains(metallic),path,"Use resolution 24…128 and valid material values")
    for element in elements {try element.validate(joints:joints)}
    try CraftError.require(surfaceElements.first?.operation == .add,path,"The exterior requires an additive first element")
    let b=try shape().bounds
    try CraftError.require(CraftMath.finite(b.min,limit:200) && CraftMath.finite(b.max,limit:200) && length(b.max-b.min)<=100,path,"Anatomical source bounds must not exceed 100 m")
  }
  public func shape() throws -> Shape {
    let surface=surfaceElements
    guard let first=surface.first,first.operation == .add else {throw CraftError(path:"anatomy.\(id)",reason:"An exterior must begin with an addition")}
    var result=first.shape
    for element in surface.dropFirst() {
      if element.operation == .cut {result=result.cut(element.shape,radius:element.cutBlend ?? 0)}
      else if element.blend>0 {result=result.blended(element.shape,radius:element.blend)}
      else {result=result.joined(element.shape)}
    }
    return result
  }
  /// Material and weight interpolation follows the same smooth CSG partition as
  /// geometry. A cut owns its visible region, but inherits skin weights when it
  /// has no explicit weight assignment, avoiding unweighted socket interiors.
  public func sample(at point:V3)->AnatomySample {
    let surface=surfaceElements
    guard let first=surface.first else {return AnatomySample(field:FieldSample(.infinity,.zero),element:"",region:"",color:.zero,jointWeights:[:])}
    func weights(_ element:AnatomyElement)->[String:Float] {element.jointWeights.isEmpty ? [part:1]:element.jointWeights}
    var result=AnatomySample(field:first.shape.sample(at:point),element:first.id,region:first.region,color:first.color,jointWeights:weights(first))
    for element in surface.dropFirst() {
      let next=element.shape.sample(at:point)
      if element.operation == .cut {
        if let k=element.cutBlend,k>0 {
          let old=result.field,h=clamp(0.5+0.5*(old.value+next.value)/k,0,1)
          result.field=FieldSample(-next.value+(old.value+next.value)*h+k*h*(1-h),-next.gradient+(old.gradient+next.gradient)*h)
          result.color=element.color+(result.color-element.color)*h
          if !element.jointWeights.isEmpty {
            var merged=result.jointWeights.mapValues{$0*h}
            for (id,weight) in element.jointWeights {merged[id,default:0]+=weight*(1-h)}
            result.jointWeights=merged.filter{$0.value>0.0000001}
          }
          if h<0.5 {result.element=element.id;result.region=element.region}
        } else if -next.value>result.field.value {
          result.field=FieldSample(-next.value,-next.gradient);result.element=element.id;result.region=element.region;result.color=element.color
          if !element.jointWeights.isEmpty {result.jointWeights=element.jointWeights}
        }
      } else {
        let h=element.blend>0 ? clamp(0.5+0.5*(next.value-result.field.value)/element.blend,0,1):(result.field.value<=next.value ? Float(1):Float(0))
        let skinRadius=max(element.blend,skinBlend)
        let skinH=skinRadius>0 ? clamp(0.5+0.5*(next.value-result.field.value)/skinRadius,0,1):h
        let old=result.field
        result.field=FieldSample(mix(next.value,old.value,h)-element.blend*h*(1-h),next.gradient+(old.gradient-next.gradient)*h)
        result.color=element.color+(result.color-element.color)*h
        var merged=result.jointWeights.mapValues{$0*skinH}
        for (id,weight) in weights(element) {merged[id,default:0]+=weight*(1-skinH)}
        result.jointWeights=merged.filter{$0.value>0.0000001}
        if h<0.5 {result.element=element.id;result.region=element.region}
      }
    }
    return result
  }
  public func anchor(id:String,at point:V3) throws -> AnatomyAnchor {
    try validate()
    try CraftError.require(!id.isEmpty && CraftMath.finite(point,limit:200),"anatomy.anchor","Use a stable ID and finite bind position")
    let sample=sample(at:point)
    guard let element=elements.first(where:{$0.id==sample.element}) else {throw CraftError(path:"anatomy.anchor",reason:"No source element at point")}
    return AnatomyAnchor(id:id,source:self.id,part:part,element:element.id,region:element.region,
      primitive:element.primitive,operation:element.operation,coordinate:element.coordinate(at:point),
      sourcePosition:point,sourceNormal:sample.field.normal)
  }
  public func changes(from previous:AnatomySource)->AnatomyChangeReport {
    var reasons:[String]=[]
    if id != previous.id || part != previous.part {reasons.append("Source or part identity changed")}
    if Set(replaces) != Set(previous.replaces) {reasons.append("Continuous surface output membership changed")}
    if elements.map(\.id) != previous.elements.map(\.id) {reasons.append("Elements were added, removed or reordered")}
    for old in previous.elements {
      guard let new=elements.first(where:{$0.id==old.id}) else {continue}
      if new.region != old.region || new.primitive != old.primitive || new.operation != old.operation || new.role != old.role {
        reasons.append("Element \(old.id) changed region, primitive, operation or role")
      }
      if Set(new.jointWeights.keys) != Set(old.jointWeights.keys) {reasons.append("Element \(old.id) changed its joint assignment")}
    }
    return AnatomyChangeReport(rebuildRequired:self != previous,rebindRequired:!reasons.isEmpty,reasons:reasons)
  }
}

public struct AnatomyChangeReport: Codable, Equatable, Sendable {
  public var rebuildRequired:Bool
  public var rebindRequired:Bool
  public var reasons:[String]
}

/// A groom root, attachment, surface-detail sample or corrective handle can own
/// one of these records. Keeping it with authored source avoids vertex-ID drift.
public struct AnatomyAnchor: Codable, Equatable, Sendable {
  public var id:String
  public var source:String
  public var part:String
  public var element:String
  public var region:String
  public var primitive:AnatomyPrimitive
  public var operation:AnatomyOperation
  public var coordinate:V3
  public var sourcePosition:V3
  public var sourceNormal:V3
  public init(id:String,source:String,part:String,element:String,region:String,primitive:AnatomyPrimitive,
    operation:AnatomyOperation,coordinate:V3,sourcePosition:V3,sourceNormal:V3) {
    self.id=id;self.source=source;self.part=part;self.element=element;self.region=region
    self.primitive=primitive;self.operation=operation;self.coordinate=coordinate
    self.sourcePosition=sourcePosition;self.sourceNormal=sourceNormal
  }
}
