import FieldCompiler
import FieldCore
import Foundation
import simd

/// Editable field source, not baked geometry. Shared by games and the workshop.
package struct FieldExpression: Codable {
  package var op: String
  package var values: [Float] = []
  package var children: [FieldExpression] = []
  package init(op: String, values: [Float] = [], children: [FieldExpression] = []) {
    self.op = op
    self.values = values
    self.children = children
  }
  package var nodeCount: Int { 1 + children.reduce(0) { $0 + $1.nodeCount } }
  package func shape(depth: Int = 0) throws -> Shape {
    guard depth < 24, values.allSatisfy(\.isFinite), values.allSatisfy({ abs($0) <= 100 }),
      children.count <= 2
    else { throw RuntimeError.message("Field exceeds depth, child or coordinate limits") }
    func require(_ n: Int, _ c: Int) throws {
      guard values.count == n, children.count == c else {
        throw RuntimeError.message("Invalid arguments for field operation \(op)")
      }
    }
    func vector(_ offset: Int = 0) -> V3 {
      V3(values[offset], values[offset + 1], values[offset + 2])
    }
    let result: Shape
    switch op {
    case "sphere":
      try require(1, 0)
      result = .sphere(values[0])
    case "box":
      try require(3, 0)
      result = .box(vector())
    case "capsule":
      try require(7, 0)
      result = .capsule(vector(), vector(3), values[6])
    case "torus":
      try require(2, 0)
      result = .torus(values[0], values[1])
    case "move":
      try require(3, 1)
      result = try children[0].shape(depth: depth + 1).moved(vector())
    case "stretch":
      try require(3, 1)
      result = try children[0].shape(depth: depth + 1).stretched(vector())
    case "scale":
      try require(1, 1)
      result = try children[0].shape(depth: depth + 1).sized(values[0])
    case "union", "subtract", "blend":
      try require(op == "blend" ? 1 : 0, 2)
      let a = try children[0].shape(depth: depth + 1)
      let b = try children[1].shape(depth: depth + 1)
      result =
        op == "union"
        ? a.joined(b) : (op == "subtract" ? a.cut(b) : a.blended(b, radius: values[0]))
    default: throw RuntimeError.message("Unknown field operation \(op)")
    }
    try result.validate()
    return result
  }
}
package struct AssetPart: Codable {
  package var name: String
  package var joint: PartJoint?
  package init(
    name: String, joint: PartJoint? = nil, field: FieldExpression, color: [Float],
    material: Int = 7, roughness: Float = 0.7, metallic: Float = 0
  ) {
    self.name = name
    self.joint = joint
    self.field = field
    self.color = color
    self.material = material
    self.roughness = roughness
    self.metallic = metallic
  }
  package var key: String { joint?.id ?? name }
  package var field: FieldExpression
  package var color: [Float]
  package var material: Int = 7
  package var roughness: Float = 0.7
  package var metallic: Float = 0
}
package struct PartOverride: Codable {
  package init(joint: PartJoint? = nil, roughness: Float? = nil, metallic: Float? = nil) {
    self.joint = joint
    self.roughness = roughness
    self.metallic = metallic
  }
  package var joint: PartJoint?
  package var roughness: Float?
  package var metallic: Float?
}
package struct AssetSource: Codable {
  package init(
    id: String, name: String, generator: String, parameters: [String: Float] = [:],
    parts: [AssetPart] = [], appearance: [String: Float] = [:], motion: MotionParameters? = nil,
    partOverrides: [String: PartOverride] = [:]
  ) {
    self.id = id
    self.name = name
    self.generator = generator
    self.parameters = parameters
    self.parts = parts
    self.appearance = appearance
    self.motion = motion
    self.partOverrides = partOverrides
  }
  package var craft = CreatureCraft()
  package var surfaceLayers:[SurfaceLayer] = []
  package var resolvedSurfaceLayers:[SurfaceLayer] {craft.boundSurfaceLayers(surfaceLayers)}
  package var resolvedCorrectives:[PoseCorrective] {craft.boundCorrectives}
  package var secondary = SecondarySettings()
  package var guideOffsets:[String:V3] = [:]
  package var guideRadii:[String:Float] = [:]
  package var guideFrames:[String:GuideFrameMode] = [:]
  package var replacedAnatomyParts:Set<String> {Set(craft.anatomy.flatMap(\.replaces))}
  /// Rest-shape deltas also deform registered guide meshes which are retained
  /// from a generator's base compilation rather than rebuilt by CraftGroom.
  package var resolvedGuideOffsets:[String:V3] {
    guard !craft.anatomyBindings.isEmpty else {return guideOffsets}
    var original=animation?.secondaryRig?(parameters) ?? SecondaryRig()
    for chain in craft.guideChains {original.append(chain.rig)}
    for panel in craft.clothPanels {original.append(panel.rig)}
    let positions=Dictionary(uniqueKeysWithValues:original.nodes.map{($0.id,$0.position)})
    return Dictionary(uniqueKeysWithValues:resolvedSecondaryRig.nodes.compactMap{node in
      guard let old=positions[node.id] else {return nil}
      return (node.id,node.position-old)
    })
  }
  package var resolvedSecondaryRig: SecondaryRig {
    var rig=animation?.secondaryRig?(parameters) ?? SecondaryRig()
    for chain in craft.boundGuideChains {rig.append(chain.rig)}
    for panel in craft.clothPanels {rig.append(panel.rig)}
    for i in rig.nodes.indices {
      rig.nodes[i].position=craft.boundPosition(rig.nodes[i].position,kind:.guideNode,target:rig.nodes[i].id)
      rig.nodes[i].position += guideOffsets[rig.nodes[i].id] ?? .zero
      rig.nodes[i].radius=guideRadii[rig.nodes[i].id] ?? rig.nodes[i].radius
      rig.nodes[i].frame=guideFrames[rig.nodes[i].id] ?? rig.nodes[i].frame
    }
    return rig
  }
  package var collisionBodies: [RigCapsule]?
  package var resolvedColliders: [RigCapsule] {collisionBodies ?? animation?.collisionBodies ?? []}
  package var surfaceEdits: [SurfaceEdit] = []
  package var contactOffsets: [String: V3] = [:]
  package var performance: PerformanceScore?
  package var version: Int = 1
  package var id: String
  package var name: String
  package var generator: String
  package var parameters: [String: Float] = [:]
  package var parts: [AssetPart] = []
  package var appearance: [String: Float] = [:]
  package var motion: MotionParameters?
  package var partOverrides: [String: PartOverride] = [:]
  package var definition: AssetGenerator? { ProjectContext.generator(generator) }
  package var controls: [ScalarControl] { definition?.controls ?? [] }
  package var authoredAnimation:AuthoredAnimation {AuthoredAnimation(craft:craft,joints:joints)}
  package var animation: AnimationDefinition? {
    var result=authoredAnimation.resolve(definition?.animation)
    if result != nil {
      let replacements=Set(craft.contactChains.map(\.id))
      result!.contactChains.removeAll{replacements.contains($0.id)}
      result!.contactChains += craft.contactChains
    }
    return result
  }
  package var shapeRanges: [String: ClosedRange<Float>] {
    Dictionary(uniqueKeysWithValues: controls.map { ($0.key, $0.range) })
  }
  package var resolvedParts: [AssetPart] {
    var result = definition?.parts(parameters).nilIfEmpty ?? parts
    for source in craft.anatomy where !result.contains(where:{$0.key==source.part}) {
      result.append(AssetPart(name:source.part,joint:PartJoint(id:source.part),
        field:FieldExpression(op:"sphere",values:[0.001]),color:[1,1,1],material:source.material,
        roughness:source.roughness,metallic:source.metallic))
    }
    for i in result.indices {
      if let override = partOverrides[result[i].key] {
        if let joint = override.joint { result[i].joint = joint }
        if let value = override.roughness { result[i].roughness = value }
        if let value = override.metallic { result[i].metallic = value }
      }
    }
    return result
  }
  package var joints: [PartJoint] { craft.resolvedJoints(resolvedParts.map { $0.joint ?? PartJoint(id: $0.name) }) }
  package static var availableIDs: [String] {
    ids
      + (((try? FileManager.default.contentsOfDirectory(
        at: url(ProjectContext.current.defaultSubject).deletingLastPathComponent(),
        includingPropertiesForKeys: nil)) ?? []).filter { $0.pathExtension == "json" }.map {
        $0.deletingPathExtension().lastPathComponent
      }.filter {
        !ids.contains($0)
      }).sorted()
  }
  package static var ids: [String] { ProjectContext.current.generators.map(\.id) }
  package static var workspace: URL { ProjectContext.workspace }
  package static func url(_ id: String) -> URL {
    ProjectContext.current.authoring.appendingPathComponent("Assets/\(id).json")
  }
  package static func defaults(_ id: String) -> AssetSource {
    let d = ProjectContext.generator(id)
    return AssetSource(
      id: id, name: d?.name ?? id, generator: id,
      parameters: Dictionary(
        uniqueKeysWithValues: (d?.controls ?? []).map { ($0.key, $0.initial) }),
      motion: d?.animation.map {
        MotionParameters(Dictionary(uniqueKeysWithValues: $0.controls.map { ($0.key, $0.initial) }))
      })
  }
  package static func load(_ id: String) throws -> AssetSource {
    guard availableIDs.contains(id) else { throw RuntimeError.message("Unknown subject \(id)") }
    if FileManager.default.fileExists(atPath: url(id).path) { return try read(url(id)) }
    return defaults(id)
  }
  package static let maximumSourceBytes=4_194_304
  package static func read(_ url: URL) throws -> AssetSource {try decode(Data(contentsOf:url))}
  package static func decode(_ data:Data) throws -> AssetSource {
    guard data.count < maximumSourceBytes else {
      throw RuntimeError.message("Asset source must be smaller than 4 MiB")
    }
    if let raw=try JSONSerialization.jsonObject(with:data) as? [String:Any],let craft=raw["craft"] {try CraftSchema.validate(craft)}
    let source = try JSONDecoder().decode(AssetSource.self, from: data)
    try source.validate()
    return source
  }
  package func validate() throws {
    guard try JSONEncoder().encode(self).count<Self.maximumSourceBytes else {throw RuntimeError.message("Asset source exceeds 4 MiB")}
    try secondary.validate()
    try resolvedSecondaryRig.validate()
    let guideIDs=Set(resolvedSecondaryRig.nodes.map(\.id))
    guard guideFrames.keys.allSatisfy(guideIDs.contains) else {throw RigError.invalid}
    guard guideRadii.allSatisfy({id,radius in guideIDs.contains(id) && radius.isFinite && (0.001...1).contains(radius)}) else {throw RigError.invalid}
    guard guideOffsets.allSatisfy({key,v in guideIDs.contains(key) && (0..<3).allSatisfy{v[$0].isFinite && abs(v[$0])<=2}}) else {throw RigError.invalid}
    guard version == 1, !id.isEmpty, id.count < 64,
      id.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") }),
      !name.isEmpty, name.count < 100
    else { throw RuntimeError.message("Invalid asset identity or version") }
    guard Self.ids.contains(generator) || generator == "fields" else {
      throw RuntimeError.message("Unknown asset generator")
    }
    for (key, value) in parameters {
      guard let range = shapeRanges[key], value.isFinite, range.contains(value) else {
        throw RuntimeError.message("Invalid shape parameter \(key)")
      }
    }
    for (key, value) in appearance {
      guard ["roughness", "metallic", "tint"].contains(key), value.isFinite,
        key == "tint" ? (0.3...1.5).contains(value) : (0...1).contains(value)
      else { throw RuntimeError.message("Invalid asset appearance") }
    }
    let ids = Set(resolvedParts.map(\.key))
    guard surfaceLayers.count<=64,Set(surfaceLayers.map(\.id)).count==surfaceLayers.count else {throw RigError.invalid}
    for layer in surfaceLayers {
      try layer.validate()
      guard ids.contains(layer.part),surfaceLayers.filter({$0.part==layer.part}).count<=16 else {throw RigError.invalid}
    }
    for binding in craft.anatomyBindings where binding.kind == .surfaceLayer {
      try CraftError.require(surfaceLayers.contains{$0.id==binding.target},"anatomyBinding.\(binding.id)","Unknown surfaceLayer target")
    }
    for layer in resolvedSurfaceLayers {try layer.validate()}
    let jointIDs=Set(joints.map(\.id))
    guard resolvedSecondaryRig.nodes.allSatisfy({jointIDs.contains($0.joint) && !jointIDs.contains($0.id)}) else {throw RigError.invalid}
    guard partOverrides.keys.allSatisfy(ids.contains),
      partOverrides.allSatisfy({ id, value in value.joint == nil || value.joint?.id == id })
    else {
      throw RuntimeError.message("Part overrides must name existing, stable anatomy IDs")
    }
    if let motion {
      guard let animation else {
        throw RuntimeError.message("This generator does not define animation")
      }
      try animation.validate(motion)
    }
    guard surfaceEdits.count <= 64, Set(surfaceEdits.map(\.id)).count == surfaceEdits.count else {throw RigError.invalid}
    for edit in surfaceEdits {try edit.validate();guard ids.contains(edit.part), (edit.targets ?? []).allSatisfy(ids.contains) else {throw RigError.invalid}}
    let chainIDs=Set(animation?.contactChains.map(\.id) ?? [])
    guard contactOffsets.allSatisfy({key,v in chainIDs.contains(key) && (0..<3).allSatisfy {v[$0].isFinite && abs(v[$0])<=0.5}}) else {throw RigError.invalid}
    let colliders=resolvedColliders, bodyIDs=Set(colliders.map(\.id))
    guard colliders.count<=32,bodyIDs.count==colliders.count else {throw RigError.invalid}
    for body in colliders {
      try body.validate()
      guard jointIDs.contains(body.joint),body.ignore.allSatisfy(bodyIDs.contains) else {throw RigError.invalid}
    }
    try craft.validate(parts:ids,rig:joints,guides:guideIDs,chains:chainIDs,clips:Set(animation?.clips ?? []))
    try PartRig.validate(joints)
    if let performance {
      guard animation?.clips.contains(performance.clip) == true else { throw PerformanceError.invalid }
      try performance.validate(joints: joints.map(\.id))
    }
    if definition?.compile == nil {
      let parts = resolvedParts
      guard (1...32).contains(parts.count), parts.reduce(0, { $0 + $1.field.nodeCount }) <= 256
      else {
        throw RuntimeError.message("A field asset needs 1...32 parts and at most 256 field nodes")
      }
      for part in parts {
        guard part.color.count == 3, part.color.allSatisfy({ $0.isFinite && (0...1).contains($0) }),
          (0...9).contains(part.material), (0.05...1).contains(part.roughness),
          (0...1).contains(part.metallic)
        else { throw RuntimeError.message("Invalid part material") }
        let shape = try part.field.shape()
        let b = shape.bounds
        guard length(b.max - b.min) <= 100, length(b.max - b.min) >= 0.001 else {
          throw RuntimeError.message("Field bounds must span 1 mm...100 m")
        }
      }
    }
  }
  package func compile() throws -> [SceneBatch] { try finish(compileBase()) }
  package func compileBase() throws -> [SceneBatch] {
    var base=try compileParts()
    for source in craft.anatomy where !base.contains(where:{$0.name==source.part}) {
      base.append(SceneBatch(name:source.part,mesh:Mesh(),instances:[Instance(tint:V3(repeating:1),kind:Float(source.material))]))
    }
    return base
  }
  package func finish(_ base: [SceneBatch],anatomyCompiler:((AnatomySource)throws->AnatomyCompilation)?=nil) throws -> [SceneBatch] {
    try base.filter{!replacedAnatomyParts.contains($0.name)}.map { part in
      var b = part
      if let anatomy=craft.anatomy.first(where:{$0.part==b.name}) {
        let compiled=try anatomyCompiler?(anatomy) ?? AnatomyCompiler.compile(anatomy,captureAnchors:false)
        b.mesh=compiled.mesh;b.skinJoints=compiled.joints;b.skinWeights=compiled.weights;b.lodMeshes=[]
        // The replacement field source owns its extraction policy. Do not
        // inherit a rigid-only policy from the discarded generator surface.
        b.surfaceReduction=nil
        b.instances=[Instance(tint:V3(repeating:1),kind:Float(anatomy.material))]
        b.roughness=anatomy.roughness;b.metallic=anatomy.metallic
      }
      if let panel=craft.clothPanels.first(where:{$0.part==b.name}) {
        (b.mesh,b.skinWeights,b.skinJoints)=try ClothCompiler.compile(panel,rig:resolvedSecondaryRig);b.lodMeshes=[]
      } else if let groom=craft.grooms.first(where:{$0.part==b.name}) {
        (b.mesh,b.skinWeights,b.skinJoints)=try CraftGroom.compile(groom,rig:resolvedSecondaryRig)
        b.lodMeshes=[]
      } else {
        b.mesh=GuideDeformation.apply(b.mesh,weights:b.skinWeights,joints:b.skinJoints,offsets:resolvedGuideOffsets)
      }
      if let levels=craft.refinement[b.name],levels>0 {
        (b.mesh,b.skinWeights)=try SculptCompiler.refine(b.mesh,weights:b.skinWeights,levels:levels);b.lodMeshes=[]
      }
      for detail in craft.detailPatches where detail.part==b.name {
        (b.mesh,b.skinWeights)=try SculptCompiler.refineLocal(b.mesh,weights:b.skinWeights,detail:detail);b.lodMeshes=[]
      }
      let edits=surfaceEdits.filter {$0.part==b.name || ($0.targets ?? []).contains(b.name)}
      for edit in edits {b.mesh=try SurfaceEditing.apply([edit],to:b.mesh,requireCoverage:edit.part==b.name)}
      let strokes=(craft.strokes+craft.sculptLayers.flatMap(\.activeStrokes)).filter{$0.part==b.name}
      b.mesh=try SculptCompiler.apply(strokes,to:b.mesh)
      let fields=craft.boundSkinFields.filter{$0.part==b.name}
      (b.skinJoints,b.skinWeights)=try SkinFieldCompiler.apply(fields,mesh:b.mesh,joints:b.skinJoints,weights:b.skinWeights,fallback:b.name)
      let seams=craft.seams.filter{$0.part==b.name}
      if !seams.isEmpty {(b.mesh,b.skinWeights)=try CostumeCompiler.compile(seams,host:b.mesh,weights:b.skinWeights)}
      if !edits.isEmpty || !strokes.isEmpty || !fields.isEmpty || !seams.isEmpty {b.lodMeshes=[]}
      // Validate corrective coverage/folds at full activation before publication.
      let correctiveFields=resolvedCorrectives.filter{$0.field.part==b.name}.map(\.field)
      if !correctiveFields.isEmpty {_ = try SurfaceEditing.apply(correctiveFields,to:b.mesh)}
      if let reduction=b.surfaceReduction {
        try CraftError.require(b.skinWeights.isEmpty && reduction.ratio.isFinite && (0.05...1).contains(reduction.ratio)
          && reduction.error.isFinite && (0.00001...0.05).contains(reduction.error),"surfaceReduction.\(b.name)",
          "Rigid reduction requires an unskinned surface, ratio 0.05…1 and absolute error 0.00001…0.05 m; remove this policy before authoring skin weights")
        b.mesh=MeshProcessing.simplify(b.mesh,ratio:reduction.ratio,error:reduction.error)
        b.lodMeshes=[]
      }
      b.doubleSided = b.doubleSided || b.instances.first?.tint.w == 8 || b.instances.first?.tint.w == 3
      if let rough = appearance["roughness"] { b.roughness = max(0.05, rough) }
      if let metal = appearance["metallic"] { b.metallic = metal }
      for i in b.instances.indices {
        let kind = b.instances[i].tint.w
        b.instances[i].tint *= appearance["tint"] ?? 1
        b.instances[i].tint.w = kind
      }
      return b
    }
  }
  private func compileParts() throws -> [SceneBatch] {
    try validate()
    func batch(
      _ name: String, _ shape: Shape, _ tint: V3, _ kind: Float, _ roughness: Float = -1,
      _ metallic: Float = 0
    ) throws -> SceneBatch {
      var b = SceneBatch(
        name: name, mesh: try Mesher.compile(shape, resolution: 48),
        instances: [Instance(tint: tint, kind: kind)])
      b.roughness = roughness
      b.metallic = metallic
      return b
    }
    if let compile = definition?.compile { return try compile(self) }
    return try resolvedParts.filter {p in !craft.anatomy.contains{$0.part==p.key}}.map { p in
      try batch(
        p.key, p.field.shape(), V3(p.color[0], p.color[1], p.color[2]), Float(p.material),
        p.roughness, p.metallic)
    }
  }

}

// Omitted optional JSON fields take the same defaults as Swift-authored fields.
extension FieldExpression {
  enum CodingKeys: String, CodingKey { case op, values, children }
  package init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    op = try c.decode(String.self, forKey: .op)
    values = try c.decodeIfPresent([Float].self, forKey: .values) ?? []
    children = try c.decodeIfPresent([Self].self, forKey: .children) ?? []
  }
}
extension AssetPart {
  enum CodingKeys: String, CodingKey {
    case name, joint, field, color, material, roughness, metallic
  }
  package init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    name = try c.decode(String.self, forKey: .name)
    joint = try c.decodeIfPresent(PartJoint.self, forKey: .joint)
    field = try c.decode(FieldExpression.self, forKey: .field)
    color = try c.decodeIfPresent([Float].self, forKey: .color) ?? [0.55, 0.55, 0.55]
    material = try c.decodeIfPresent(Int.self, forKey: .material) ?? 7
    roughness = try c.decodeIfPresent(Float.self, forKey: .roughness) ?? 0.7
    metallic = try c.decodeIfPresent(Float.self, forKey: .metallic) ?? 0
  }
}
extension AssetSource {
  enum CodingKeys: String, CodingKey {
    case version, id, name, generator, parameters, parts, appearance, motion, partOverrides, performance, surfaceEdits, contactOffsets, collisionBodies, secondary, guideOffsets, guideRadii, guideFrames, surfaceLayers, craft
  }
  package init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: CodingKeys.self)
    version = try c.decodeIfPresent(Int.self, forKey: .version) ?? 1
    id = try c.decode(String.self, forKey: .id)
    name = try c.decode(String.self, forKey: .name)
    generator = try c.decodeIfPresent(String.self, forKey: .generator) ?? "fields"
    parameters = try c.decodeIfPresent([String: Float].self, forKey: .parameters) ?? [:]
    parts = try c.decodeIfPresent([AssetPart].self, forKey: .parts) ?? []
    partOverrides =
      try c.decodeIfPresent([String: PartOverride].self, forKey: .partOverrides) ?? [:]
    craft = try c.decodeIfPresent(CreatureCraft.self,forKey:.craft) ?? CreatureCraft()
    surfaceLayers = try c.decodeIfPresent([SurfaceLayer].self,forKey:.surfaceLayers) ?? []
    guideOffsets = try c.decodeIfPresent([String:V3].self,forKey:.guideOffsets) ?? [:]
    guideRadii = try c.decodeIfPresent([String:Float].self,forKey:.guideRadii) ?? [:]
    guideFrames = try c.decodeIfPresent([String:GuideFrameMode].self,forKey:.guideFrames) ?? [:]
    secondary = try c.decodeIfPresent(SecondarySettings.self,forKey:.secondary) ?? SecondarySettings()
    collisionBodies = try c.decodeIfPresent([RigCapsule].self,forKey:.collisionBodies)
    surfaceEdits = try c.decodeIfPresent([SurfaceEdit].self, forKey: .surfaceEdits) ?? []
    contactOffsets = try c.decodeIfPresent([String:V3].self, forKey: .contactOffsets) ?? [:]
    performance = try c.decodeIfPresent(PerformanceScore.self, forKey: .performance)
    motion = try c.decodeIfPresent(MotionParameters.self, forKey: .motion)
    appearance = try c.decodeIfPresent([String: Float].self, forKey: .appearance) ?? [:]
  }
}

extension Array { fileprivate var nilIfEmpty: Self? { isEmpty ? nil : self } }

// One pose evaluator: authored pose, additive corrections, then contact constraints.
extension AssetSource {
  package func evaluatedPose(clip: String, time: Float, state: BehaviorSnapshot,
    contacts: Bool = true, height: (Float,Float)->Float = {_,_ in 0},
    normal: (Float,Float)->V3 = {_,_ in V3(0,1,0)}) -> ContactResult {
    let authored=authoredPose(clip:clip,time:time,state:state),resolvedJoints=joints
    var supportWeights:[String:Float]=[:]
    if let a=craft.arrangement,a.clip==clip,let control=craft.balance {
      let t=a.looping ? max(0,time).truncatingRemainder(dividingBy:a.duration):max(0,min(a.duration,time))
      for layer in a.layers where !layer.additive {
        let s=layer.sample(t)
        guard s.weight>0,let phrase=craft.phrases.first(where:{$0.id==layer.phrase}) else {continue}
        let sourceRate=(layer.sourceEnd-layer.sourceStart)/(layer.end-layer.start)
        for path in phrase.contacts {supportWeights[path.chain]=path.supportWeight(s.time,transition:(control.transitionSeconds ?? 0)*sourceRate)}
      }
    }
    let supportTargets=Dictionary(uniqueKeysWithValues:authored.contacts.map{id,target in (id,ContactTarget(target.position+(contactOffsets[id] ?? .zero),planted:target.planted))})
    let poses=clip != "bind" && contacts ? craft.balance.map{BodySupport.assist($0,joints:resolvedJoints,poses:authored.poses,masses:craft.masses,targets:supportTargets,supportWeights:supportWeights)} ?? authored.poses:authored.poses
    let matrices=PartRig.matrices(resolvedJoints,poses:poses)
    guard clip != "bind",let animation else {return ContactResult(matrices:matrices,contacts:[])}
    return ContactRig.solve(joints:resolvedJoints,matrices:matrices,chains:animation.contactChains,
      targets:authored.contacts,offsets:contactOffsets,height:height,normal:normal,applying:contacts)
  }
  package func authoredPose(clip:String,time:Float,state:BehaviorSnapshot)->(poses:[String:JointPose],contacts:[String:ContactTarget]) {
    guard clip != "bind" else {return ([:],[:])}
    let m=motion ?? MotionParameters()
    var poses=animation?.poses(clip,time,state,m) ?? [:]
    if let score=performance,score.clip==clip {poses=score.apply(to:poses,time:time)}
    let targets=animation?.contactTargets?(clip,time,state,m) ?? [:]
    if let arrangement=craft.arrangement,arrangement.clip==clip {
      return arrangement.evaluate(time:time,phrases:craft.phrases,poses:poses,contacts:targets)
    }
    if let phrase=craft.phrases.first(where:{$0.id==clip}) {
      let t=max(0,min(phrase.duration,time))
      return (poses.merging(phrase.pose(at:t)){_,authored in authored},
        targets.merging(Dictionary(uniqueKeysWithValues:phrase.contacts.map{($0.chain,$0.sample(t))})){_,authored in authored})
    }
    return (poses,targets)
  }
  package func correctiveUniforms(part:String,clip:String,time:Float,state:BehaviorSnapshot)->[CorrectiveUniform] {
    guard clip != "bind",craft.correctives.contains(where:{$0.field.part==part}) else {return []}
    let pose=authoredPose(clip:clip,time:time,state:state).poses
    return resolvedCorrectives.filter{$0.field.part==part}.map {c in CorrectiveUniform(c.field,weight:c.activation(pose[c.driver] ?? JointPose()))}
  }
  package func poseMatrices(clip: String, time: Float, state: BehaviorSnapshot) -> [String:simd_float4x4] {
    evaluatedPose(clip:clip,time:time,state:state).matrices
  }
}
