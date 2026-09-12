import Foundation
import simd

public struct GroomDesign: Codable, Equatable, Sendable {
  public var id:String
  public var part:String
  public var guides:[[String]]
  public var fibres:Int
  public var width:Float
  public var radius:Float
  public var clump:Float
  public var curl:Float
  public var frequency:Float
  public var lengthVariation:Float
  public var flyaways:Float
  public var seed:UInt32
  public var rootColor:V3
  public var tipColor:V3
  public var envelope:GroomEnvelope?
  public init(id:String,part:String,guides:[[String]],fibres:Int=48,width:Float=0.06,radius:Float=0.003,
    clump:Float=0.8,curl:Float=0.02,frequency:Float=2,lengthVariation:Float=0.2,flyaways:Float=0.05,
    seed:UInt32=17,rootColor:V3=V3(0.35,0.24,0.10),tipColor:V3=V3(0.6,0.46,0.24),envelope:GroomEnvelope?=nil) {
    self.id=id;self.part=part;self.guides=guides;self.fibres=fibres;self.width=width;self.radius=radius
    self.clump=clump;self.curl=curl;self.frequency=frequency;self.lengthVariation=lengthVariation
    self.flyaways=flyaways;self.seed=seed;self.rootColor=rootColor;self.tipColor=tipColor;self.envelope=envelope
  }
  public func validate(nodes:Set<String>) throws {
    try envelope?.validate(id:id)
    let ids=guides.flatMap{$0}
    try CraftError.require(!id.isEmpty && id.count<=80 && !guides.isEmpty && guides.count<=16
      && guides.allSatisfy{(2...16).contains($0.count)} && Set(ids).count==ids.count && ids.count<=64 && ids.allSatisfy(nodes.contains)
      && (1...128).contains(fibres),"groom.\(id)","Use 1…16 guides with 2…16 unique nodes each, at most 64 total, and 1…128 fibres per guide")
    try CraftError.require([width,radius,clump,curl,frequency,lengthVariation,flyaways].allSatisfy(\.isFinite)
      && (0.001...0.5).contains(width) && (0.0002...0.02).contains(radius) && (0...1).contains(clump)
      && (0...0.2).contains(curl) && (0...12).contains(frequency) && (0...0.8).contains(lengthVariation)
      && (0...0.5).contains(flyaways) && [rootColor,tipColor].allSatisfy{CraftMath.finite($0,limit:1) && $0.min()>=0},
      "groom.\(id)","Invalid fibre dimensions, shape or colors")
  }
}
public struct CostumeSeam: Codable, Equatable, Sendable {
  public var id:String
  public var part:String
  public var points:[V3]
  public var radius:Float
  public var spacing:Float
  public var lift:Float
  public var color:V3
  public init(id:String,part:String,points:[V3],radius:Float=0.004,spacing:Float=0.02,lift:Float=0.003,color:V3=V3(0.6,0.45,0.2)) {
    self.id=id;self.part=part;self.points=points;self.radius=radius;self.spacing=spacing;self.lift=lift;self.color=color
  }
}
public struct BodyMass: Codable, Equatable, Sendable {
  public var joint:String
  public var center:V3
  public var kilograms:Float
  public init(joint:String,center:V3,kilograms:Float) {self.joint=joint;self.center=center;self.kilograms=kilograms}
}

/// Versioned, optional source extension. Default sources have no craft effects.
/// Geometry-bearing and time-only fields are separated for compile cache keys.
public struct CreatureCraft: Codable, Equatable, Sendable {
  public var version:Int = 1
  public var anatomy:[AnatomySource] = []
  public var anatomyAnchors:[AnatomyAnchor] = []
  public var anatomyBindings:[AnatomyBinding] = []
  public var contactChains:[ContactChain] = []
  public var landmarks:[AnatomyLandmark] = []
  public var strokes:[SculptStroke] = []
  public var sculptLayers:[SculptLayer] = []
  public var detailPatches:[SculptDetail] = []
  public var refinement:[String:Int] = [:]
  public var joints:[PartJoint] = []
  public var skinFields:[SkinField] = []
  public var correctives:[PoseCorrective] = []
  public var phrases:[MotionPhrase] = []
  public var arrangement:MotionArrangement?
  public var masses:[BodyMass] = []
  public var balance:BalanceControl?
  public var guideChains:[CraftGuideChain] = []
  public var clothPanels:[ClothPanel] = []
  public var grooms:[GroomDesign] = []
  public var seams:[CostumeSeam] = []
  public init() {}
  public var geometry:CreatureCraft {
    var c=self;c.contactChains=[];c.landmarks=[];c.phrases=[];c.arrangement=nil;c.masses=[];c.balance=nil;c.joints=[];c.correctives=[];return c
  }
  public func geometry(for part:String)->CreatureCraft {
    var c=geometry;c.anatomy=c.anatomy.filter{$0.part==part};c.strokes=c.strokes.filter{$0.part==part};c.sculptLayers=c.sculptLayers.map{var l=$0;l.strokes=l.strokes.filter{$0.part==part};return l}.filter{!$0.strokes.isEmpty};c.skinFields=c.skinFields.filter{$0.part==part}
    c.detailPatches=c.detailPatches.filter{$0.part==part}
    c.refinement=c.refinement.filter{$0.key==part};c.grooms=c.grooms.filter{$0.part==part};c.seams=c.seams.filter{$0.part==part};c.clothPanels=c.clothPanels.filter{$0.part==part}
    let used=Set(c.grooms.flatMap{$0.guides.flatMap{$0}})
    c.guideChains=c.guideChains.filter{chain in chain.points.indices.contains{i in used.contains(chain.id+"-\(i)")}}
    let groomNodes=Set(c.grooms.flatMap{$0.guides.flatMap{$0}}),chainIDs=Set(c.guideChains.map(\.id))
    c.anatomyBindings=c.anatomyBindings.filter{binding in
      switch binding.kind {
      case .guideChain:return chainIDs.contains(binding.target)
      case .guideNode:return groomNodes.contains(binding.target)
      case .skinField:return c.skinFields.contains{$0.id==binding.target}
      case .corrective:return correctives.contains{$0.id==binding.target && $0.field.part==part}
      default:return false
      }
    }
    let anchors=Set(c.anatomyBindings.map(\.anchor));c.anatomyAnchors=c.anatomyAnchors.filter{anchors.contains($0.id)}
    return c
  }
  public func resolvedJoints(_ base:[PartJoint])->[PartJoint] {
    (base.map{j in joints.first{$0.id==j.id} ?? j}+joints.filter{j in !base.contains{$0.id==j.id}}).map {joint in
      var j=joint;j.offset += binding(.joint,j.id)?.displacement(anchors:anatomyAnchors) ?? .zero;return j
    }
  }
  public func validate(parts:Set<String>,rig:[PartJoint],guides:Set<String>,chains:Set<String>,clips:Set<String>) throws {
    try CraftError.require(version==1,"version","Unsupported craft source version")
    let ids=Set(rig.map(\.id));try PartRig.validate(rig)
    try CraftError.require(anatomy.count<=16 && Set(anatomy.map(\.id)).count==anatomy.count && Set(anatomy.map(\.part)).count==anatomy.count,"anatomy","Use at most 16 sources with unique IDs and part names")
    let replaced=anatomy.flatMap(\.replaces),anatomyParts=Set(anatomy.map(\.part))
    try CraftError.require(Set(replaced).count==replaced.count && replaced.allSatisfy{parts.contains($0) && !anatomyParts.contains($0)},
      "anatomy.replaces","Replaced outputs must name existing parts exactly once and cannot name any authored anatomy output")
    for source in anatomy {try source.validate(joints:ids);try CraftError.require(parts.contains(source.part),"anatomy.\(source.id)","Unknown part")}
    try CraftError.require(anatomyAnchors.count<=8192 && Set(anatomyAnchors.map(\.id)).count==anatomyAnchors.count,"anatomyAnchors","Use at most 8,192 uniquely named anchors")
    for anchor in anatomyAnchors {
      guard let source=anatomy.first(where:{$0.id==anchor.source}),let element=source.elements.first(where:{$0.id==anchor.element}) else {throw CraftError(path:"anatomyAnchor.\(anchor.id)",reason:"Unknown source or element; explicitly remove or rebind this anchor")}
      try CraftError.require(!anchor.id.isEmpty && anchor.id.count<=80 && source.part==anchor.part && element.role == .surface
        && element.region==anchor.region && element.primitive==anchor.primitive && element.operation==anchor.operation
        && CraftMath.finite(anchor.coordinate,limit:10000) && CraftMath.finite(anchor.sourcePosition,limit:200)
        && CraftMath.finite(anchor.sourceNormal,limit:1) && (0.9...1.1).contains(length(anchor.sourceNormal)),
        "anatomyAnchor.\(anchor.id)","Invalid correspondence identity or coordinates; recreate against the current source")
      try CraftError.require(length(element.point(at:anchor.coordinate)-anchor.sourcePosition)<0.001
        && abs(source.sample(at:anchor.sourcePosition).field.value)<0.002,
        "anatomyAnchor.\(anchor.id)","Anchor no longer lies on its source surface; use bounded rebind recovery")
    }
    try CraftError.require(anatomyBindings.count<=256 && Set(anatomyBindings.map(\.id)).count==anatomyBindings.count
      && Set(anatomyBindings.map{$0.kind.rawValue+"/"+$0.target}).count==anatomyBindings.count,
      "anatomyBindings","Use at most 256 unique bindings; a target may have only one binding of each kind")
    for binding in anatomyBindings {
      try binding.validate(anchors:anatomyAnchors)
      let exists:Bool
      switch binding.kind {
      case .guideChain:exists=guideChains.contains{$0.id==binding.target}
      case .guideNode:exists=guides.contains(binding.target)
      case .joint:exists=ids.contains(binding.target)
      case .landmark:exists=landmarks.contains{$0.id==binding.target}
      case .skinField:exists=skinFields.contains{$0.id==binding.target}
      case .corrective:exists=correctives.contains{$0.id==binding.target}
      case .surfaceLayer:exists=true // AssetSource validates its separate finish-layer collection.
      }
      try CraftError.require(exists,"anatomyBinding.\(binding.id)","Unknown \(binding.kind.rawValue) target \(binding.target)")
      if binding.kind == .joint {try CraftError.require(!binding.followNormal,"anatomyBinding.\(binding.id)","Joint attachments support translation; bind guide curves for a following surface frame")}
    }
    for chain in guideChains where anatomyBindings.contains(where:{$0.kind == .guideChain && $0.target==chain.id}) {
      try CraftError.require(!anatomyBindings.contains{binding in binding.kind == .guideNode && chain.points.indices.contains{binding.target==chain.id+"-\($0)"}},
        "anatomyBinding.\(chain.id)","Bind the whole chain or its individual nodes; combining both would apply the transform twice")
    }
    try CraftError.require(contactChains.count<=16 && Set(contactChains.map(\.id)).count==contactChains.count,"contactChains","Use at most 16 unique contact chains")
    for chain in contactChains {try chain.validate(joints:rig)}
    try CraftError.require(joints.count<=64 && Set(joints.map(\.id)).count==joints.count,"joints","Duplicate joints or more than 64")
    try CraftError.require(landmarks.count<=128 && Set(landmarks.map(\.id)).count==landmarks.count,"landmarks","Duplicate IDs or more than 128")
    for l in landmarks {try CraftError.require(!l.id.isEmpty && l.id.count<=80 && parts.contains(l.part) && CraftMath.finite(l.position,limit:100) && l.note.count<=2000,"landmark.\(l.id)","Invalid part, position or note")}
    try CraftError.require(strokes.count<=128 && Set(strokes.map(\.id)).count==strokes.count,"strokes","Duplicate IDs or more than 128")
    for s in strokes {try s.validate();try CraftError.require(parts.contains(s.part),"stroke.\(s.id)","Unknown part")}
    try CraftError.require(refinement.allSatisfy{parts.contains($0.key) && (0...2).contains($0.value)},"refinement","Unknown part or levels outside 0…2")
    try CraftError.require(detailPatches.count<=16 && Set(detailPatches.map(\.id)).count==detailPatches.count,"detailPatches","Use at most 16 uniquely named patches")
    for detail in detailPatches {try detail.validate();try CraftError.require(parts.contains(detail.part),"detail.\(detail.id)","Unknown part")}
    try CraftError.require(sculptLayers.count<=32 && Set(sculptLayers.map(\.id)).count==sculptLayers.count
      && sculptLayers.reduce(0){$0+$1.strokes.count}<=256,"sculptLayers","Use at most 32 unique layers and 256 strokes")
    for layer in sculptLayers {
      try CraftError.require(!layer.id.isEmpty && layer.id.count<=80 && layer.opacity.isFinite && (0...1).contains(layer.opacity)
        && Set(layer.strokes.map(\.id)).count==layer.strokes.count,"sculptLayer.\(layer.id)","Use a named layer, opacity 0…1 and unique stroke IDs")
      for stroke in layer.strokes {try stroke.validate();try CraftError.require(parts.contains(stroke.part),"sculptLayer.\(layer.id)","Unknown stroke part")}
    }
    try CraftError.require(skinFields.count<=128 && Set(skinFields.map(\.id)).count==skinFields.count,"skinFields","Duplicate IDs or more than 128")
    for s in skinFields {try s.validate(joints:ids.union(guides));try CraftError.require(parts.contains(s.part),"skinField.\(s.id)","Unknown part")}
    try CraftError.require(correctives.count<=64 && Set(correctives.map(\.id)).count==correctives.count,"correctives","Duplicate IDs or more than 64")
    for c in correctives {
      try c.validate(joints:ids)
      try CraftError.require(parts.contains(c.field.part) && (c.field.targets ?? []).isEmpty
        && correctives.filter{$0.field.part==c.field.part}.count<=16,"corrective.\(c.id)","Use one valid part and at most 16 correctives per part")
    }
    try CraftError.require(phrases.count<=32 && Set(phrases.map(\.id)).count==phrases.count,"phrases","Duplicate IDs or more than 32")
    try CraftError.require(phrases.reduce(0){$0+$1.samples.reduce(0){$0+$1.poses.count}}<=16384,"phrases.budget","At most 16,384 joint poses per source")
    for p in phrases {try p.validate(joints:ids,chains:chains)}
    try arrangement?.validate(phrases:phrases,clips:clips)
    try CraftError.require(masses.count<=64 && Set(masses.map(\.joint)).count==masses.count,"masses","Duplicate joints or more than 64")
    for m in masses {try CraftError.require(ids.contains(m.joint) && CraftMath.finite(m.center,limit:100) && m.kilograms.isFinite && (0.001...100000).contains(m.kilograms),"mass.\(m.joint)","Invalid mass or center")}
    if let b=balance {try CraftError.require(ids.contains(b.joint) && !masses.isEmpty && b.strength.isFinite && (0...1).contains(b.strength) && b.maximumShift.isFinite && (0...0.5).contains(b.maximumShift),"balance","Use an existing joint, mass model, strength 0…1 and shift 0…0.5 m")}
    if let t=balance?.transitionSeconds {try CraftError.require(t.isFinite && (0...1).contains(t),"balance.transitionSeconds","Use 0…1 seconds")}
    try CraftError.require(guideChains.count<=32 && Set(guideChains.map(\.id)).count==guideChains.count,"guideChains","Duplicate IDs or more than 32")
    for chain in guideChains {try chain.validate(joints:ids)}
    try CraftError.require(clothPanels.count<=8 && Set(clothPanels.map(\.id)).count==clothPanels.count && Set(clothPanels.map(\.part)).count==clothPanels.count,"clothPanels","Duplicate IDs/parts or more than eight panels")
    for panel in clothPanels {try panel.validate(joints:ids);try CraftError.require(parts.contains(panel.part) && !grooms.contains{$0.part==panel.part},"clothPanel.\(panel.id)","Unknown part or both groom and cloth assigned to a part")}
    try CraftError.require(grooms.count<=16 && Set(grooms.map(\.id)).count==grooms.count && Set(grooms.map(\.part)).count==grooms.count,"grooms","Duplicate ID/part or more than 16")
    for g in grooms {try g.validate(nodes:guides);try CraftError.require(parts.contains(g.part),"groom.\(g.id)","Unknown part")}
    try CraftError.require(seams.count<=64 && Set(seams.map(\.id)).count==seams.count,"seams","Duplicate IDs or more than 64")
    for s in seams {
      try CraftError.require(!s.id.isEmpty && s.id.count<=80 && parts.contains(s.part) && (2...128).contains(s.points.count)
        && s.points.allSatisfy{CraftMath.finite($0,limit:100)} && [s.radius,s.spacing,s.lift].allSatisfy(\.isFinite)
        && (0.0005...0.05).contains(s.radius) && (0.005...0.2).contains(s.spacing) && (0...0.05).contains(s.lift)
        && CraftMath.finite(s.color,limit:1) && s.color.min()>=0,"seam.\(s.id)","Invalid seam path, dimensions or color")
    }
  }
  enum CodingKeys:String,CodingKey {case detailPatches,sculptLayers,version,anatomy,anatomyAnchors,anatomyBindings,contactChains,landmarks,strokes,refinement,joints,skinFields,correctives,phrases,arrangement,masses,balance,guideChains,clothPanels,grooms,seams}
  public init(from decoder:Decoder) throws {
    try CraftSchema.validate(CraftJSON(from:decoder).value)
    let c=try decoder.container(keyedBy:CodingKeys.self)
    version=try c.decodeIfPresent(Int.self,forKey:.version) ?? 1
    anatomy=try c.decodeIfPresent([AnatomySource].self,forKey:.anatomy) ?? []
    anatomyAnchors=try c.decodeIfPresent([AnatomyAnchor].self,forKey:.anatomyAnchors) ?? []
    anatomyBindings=try c.decodeIfPresent([AnatomyBinding].self,forKey:.anatomyBindings) ?? []
    contactChains=try c.decodeIfPresent([ContactChain].self,forKey:.contactChains) ?? []
    landmarks=try c.decodeIfPresent([AnatomyLandmark].self,forKey:.landmarks) ?? []
    strokes=try c.decodeIfPresent([SculptStroke].self,forKey:.strokes) ?? []
    sculptLayers=try c.decodeIfPresent([SculptLayer].self,forKey:.sculptLayers) ?? []
    detailPatches=try c.decodeIfPresent([SculptDetail].self,forKey:.detailPatches) ?? []
    refinement=try c.decodeIfPresent([String:Int].self,forKey:.refinement) ?? [:]
    joints=try c.decodeIfPresent([PartJoint].self,forKey:.joints) ?? []
    skinFields=try c.decodeIfPresent([SkinField].self,forKey:.skinFields) ?? []
    correctives=try c.decodeIfPresent([PoseCorrective].self,forKey:.correctives) ?? []
    phrases=try c.decodeIfPresent([MotionPhrase].self,forKey:.phrases) ?? []
    arrangement=try c.decodeIfPresent(MotionArrangement.self,forKey:.arrangement)
    masses=try c.decodeIfPresent([BodyMass].self,forKey:.masses) ?? []
    balance=try c.decodeIfPresent(BalanceControl.self,forKey:.balance)
    guideChains=try c.decodeIfPresent([CraftGuideChain].self,forKey:.guideChains) ?? []
    clothPanels=try c.decodeIfPresent([ClothPanel].self,forKey:.clothPanels) ?? []
    grooms=try c.decodeIfPresent([GroomDesign].self,forKey:.grooms) ?? []
    seams=try c.decodeIfPresent([CostumeSeam].self,forKey:.seams) ?? []
  }
}

/// Retain the raw shape for strict validation even when loaded inside a study.
/// This prevents Codable's usual silent dropping of misspelled nested fields.
private struct CraftJSON:Decodable {
  let value:Any
  init(from decoder:Decoder) throws {
    try CraftError.require(decoder.codingPath.count<=32,"source.depth","Source nesting exceeds 32 levels")
    let c=try decoder.singleValueContainer()
    if c.decodeNil() {value=NSNull()}
    else if let object=try? c.decode([String:CraftJSON].self) {value=object.mapValues(\.value)}
    else if let array=try? c.decode([CraftJSON].self) {value=array.map(\.value)}
    else if let boolean=try? c.decode(Bool.self) {value=boolean}
    else if let number=try? c.decode(Double.self) {value=number}
    else {value=try c.decode(String.self)}
  }
}
