import FieldCore
import Foundation
import simd

public struct AnatomyStructure: Sendable {
  public var id:String
  public var region:String
  public var shape:Shape
  public var jointWeights:[String:Float]
}
public struct AnatomyCompileReport: Codable, Equatable, Sendable {
  public var vertices:Int
  public var triangles:Int
  public var regions:[String:Int]
  public var maximumDiscardedSkinWeight:Float
}
public struct AnatomyCompilation: Sendable {
  public var mesh:Mesh
  public var joints:[String]
  public var weights:[SkinWeight]
  public var anchors:[AnatomyAnchor]
  public var structures:[AnatomyStructure]
  public var report:AnatomyCompileReport
}

public struct AnatomyRebindPolicy: Codable, Equatable, Sendable {
  public var allowStructuralChanges:Bool
  public var maximumProjection:Float
  public var tolerance:Float
  public var elementMap:[String:String]
  public init(allowStructuralChanges:Bool=false,maximumProjection:Float=0.05,tolerance:Float=0.0002,elementMap:[String:String]=[:]) {
    self.allowStructuralChanges=allowStructuralChanges;self.maximumProjection=maximumProjection
    self.tolerance=tolerance;self.elementMap=elementMap
  }
}
public struct AnatomyRebindIssue: Codable, Equatable, Sendable {
  public var anchor:String
  public var reason:String
  public var fieldResidual:Float
  public var projectionDistance:Float
  public var recovery:String
}
public struct AnatomyRebindResult: Codable, Equatable, Sendable {
  public var accepted:Bool
  public var anchors:[AnatomyAnchor]
  public var positions:[V3]
  public var normals:[V3]
  public var maximumFieldResidual:Float
  public var maximumProjectionDistance:Float
  public var issues:[AnatomyRebindIssue]
}

/// Extracted topology is disposable. Semantic field coordinates bind skin,
/// detail, groom roots and attachments to authored volumes, then a bounded
/// projection resolves the composed surface. No nearest-vertex fallback hides
/// lost regions or lets an attachment silently jump to the opposite surface.
public enum AnatomyCompiler {
  public static func compile(_ source:AnatomySource,captureAnchors:Bool=true) throws -> AnatomyCompilation {
    try source.validate()
    let shape=try source.shape(),bounds=shape.occupiedBounds()
    var mesh=try Mesher.compile(shape,resolution:source.resolution,bounds:bounds)
    try CraftError.require(!mesh.vertices.isEmpty,"anatomy.\(source.id)","No exterior surface survived compilation; inspect the ordered cuts")
    let jointIDs=Set(source.elements.flatMap{$0.jointWeights.keys}).union(source.elements.contains{$0.role == .surface && $0.jointWeights.isEmpty && $0.operation == .add} ? [source.part]:[]).sorted()
    try CraftError.require(jointIDs.count<=64,"anatomy.\(source.id).joints","At most 64 joints may influence one compiled anatomy")
    let elementByID=Dictionary(uniqueKeysWithValues:source.elements.map{($0.id,$0)})
    var weights:[SkinWeight]=[],anchors:[AnatomyAnchor]=[],regions:[String:Int]=[:],maximumDiscarded:Float=0
    weights.reserveCapacity(mesh.vertices.count);if captureAnchors {anchors.reserveCapacity(mesh.vertices.count)}
    for i in mesh.vertices.indices {
      let v=mesh.vertices[i].position,p=V3(v.x,v.y,v.z),sample=source.sample(at:p),element=elementByID[sample.element]!
      mesh.vertices[i].color=SIMD4(sample.color,1)
      let sorted=sample.jointWeights.sorted{$0.value==$1.value ? $0.key<$1.key:$0.value>$1.value},kept=Array(sorted.prefix(4)),total=kept.reduce(Float(0)){$0+$1.value}
      maximumDiscarded=max(maximumDiscarded,sorted.dropFirst(4).reduce(0){$0+$1.value})
      var skin=SkinWeight(0);skin.weights = .zero
      for (k,pair) in kept.enumerated() {skin.joints[k]=UInt32(jointIDs.firstIndex(of:pair.key)!);skin.weights[k]=pair.value/max(total,0.0000001)}
      weights.append(skin);regions[sample.region,default:0]+=1
      if captureAnchors {anchors.append(AnatomyAnchor(id:"\(source.part)-v\(i)",source:source.id,part:source.part,element:element.id,region:element.region,
        primitive:element.primitive,operation:element.operation,coordinate:element.coordinate(at:p),sourcePosition:p,sourceNormal:sample.field.normal))}
    }
    let structures=source.internalElements.map{AnatomyStructure(id:$0.id,region:$0.region,shape:$0.shape,jointWeights:$0.jointWeights)}
    return AnatomyCompilation(mesh:mesh,joints:jointIDs,weights:weights,anchors:anchors,structures:structures,
      report:AnatomyCompileReport(vertices:mesh.vertices.count,triangles:mesh.indices.count/3,regions:regions,maximumDiscardedSkinWeight:maximumDiscarded))
  }

  /// Failure is atomic: the original anchors/positions are returned with all
  /// actionable diagnostics. Structural changes require an explicit policy;
  /// deleted regions additionally require an explicit old-to-new element map.
  public static func rebind(_ anchors:[AnatomyAnchor],from previous:AnatomySource,to candidate:AnatomySource,
    policy:AnatomyRebindPolicy=AnatomyRebindPolicy()) throws -> AnatomyRebindResult {
    try previous.validate();try candidate.validate()
    try CraftError.require(policy.maximumProjection.isFinite && (0.0001...2).contains(policy.maximumProjection)
      && policy.tolerance.isFinite && (0.000001...0.01).contains(policy.tolerance),"anatomy.rebind.policy","Use projection 0.0001…2 m and field tolerance 0.000001…0.01")
    let changes=candidate.changes(from:previous),shape=try candidate.shape()
    var rebound:[AnatomyAnchor]=[],issues:[AnatomyRebindIssue]=[],maximumResidual:Float=0,maximumDistance:Float=0
    func issue(_ anchor:AnatomyAnchor,_ reason:String,_ recovery:String,residual:Float=0,distance:Float=0) {
      issues.append(AnatomyRebindIssue(anchor:anchor.id,reason:reason,fieldResidual:residual,projectionDistance:distance,recovery:recovery))
    }
    if changes.rebindRequired && !policy.allowStructuralChanges {
      for reason in changes.reasons {issues.append(AnatomyRebindIssue(anchor:"*",reason:reason,fieldResidual:0,projectionDistance:0,recovery:"Review structural changes, provide any element replacements, then retry with allowStructuralChanges"))}
    } else {
      for anchor in anchors {
        guard anchor.source==previous.id && anchor.part==previous.part && CraftMath.finite(anchor.coordinate,limit:10000)
          && CraftMath.finite(anchor.sourcePosition,limit:200) && CraftMath.finite(anchor.sourceNormal,limit:1) else {
          issue(anchor,"Anchor source identity or coordinates are invalid","Recreate this anchor against the previous source");continue
        }
        let mapped=policy.elementMap[anchor.element] ?? anchor.element
        guard let old=previous.elements.first(where:{$0.id==anchor.element}),old.role == .surface,
          old.region==anchor.region,old.primitive==anchor.primitive,old.operation==anchor.operation else {
          issue(anchor,"Anchor does not match its previous source element","Recreate this anchor against the previous source");continue
        }
        guard let element=candidate.elements.first(where:{$0.id==mapped && $0.role == .surface}) else {
          issue(anchor,"Source element \(mapped) is absent from the candidate surface","Provide elementMap[\(anchor.element)] identifying the intended replacement");continue
        }
        if element.primitive != anchor.primitive && policy.elementMap[anchor.element]==nil {
          issue(anchor,"Primitive coordinates changed for \(anchor.element)","Explicitly map this element to authorize new primitive coordinates");continue
        }
        let initial:V3
        if element.primitive==anchor.primitive {initial=element.point(at:anchor.coordinate)}
        else {initial=anchor.sourcePosition}
        var point=initial
        for _ in 0..<32 {
          let sample=shape.sample(at:point),g2=length_squared(sample.gradient)
          if abs(sample.value)<=policy.tolerance || g2<1e-12 {break}
          var step=sample.gradient*(sample.value/g2)
          let magnitude=length(step),limit=policy.maximumProjection*0.25
          if magnitude>limit {step *= limit/magnitude}
          let next=point-step
          if length(next-initial)>policy.maximumProjection {break}
          point=next
        }
        let sample=candidate.sample(at:point),residual=abs(sample.field.value),distance=length(point-initial)
        maximumResidual=max(maximumResidual,residual);maximumDistance=max(maximumDistance,distance)
        if residual>policy.tolerance {
          issue(anchor,"Surface projection did not converge inside its allowed distance","Inspect this source coordinate, increase the bounded projection distance, or author a replacement anchor",residual:residual,distance:distance);continue
        }
        if sample.region != element.region {
          issue(anchor,"Projection reached region \(sample.region) instead of \(element.region)","Choose an explicit element replacement or restore the intended source region",residual:residual,distance:distance);continue
        }
        guard let owner=candidate.elements.first(where:{$0.id==sample.element}) else {continue}
        rebound.append(AnatomyAnchor(id:anchor.id,source:candidate.id,part:candidate.part,element:owner.id,region:owner.region,
          primitive:owner.primitive,operation:owner.operation,coordinate:owner.coordinate(at:point),sourcePosition:point,sourceNormal:sample.field.normal))
      }
    }
    let accepted=issues.isEmpty,result=accepted ? rebound:anchors
    return AnatomyRebindResult(accepted:accepted,anchors:result,positions:result.map(\.sourcePosition),normals:result.map(\.sourceNormal),
      maximumFieldResidual:maximumResidual,maximumProjectionDistance:maximumDistance,issues:issues)
  }
}
