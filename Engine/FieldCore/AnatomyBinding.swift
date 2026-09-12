import Foundation
import simd

public enum AnatomyBindingKind:String,Codable,Sendable {
  case guideChain,guideNode,joint,landmark,skinField,corrective,surfaceLayer
}
/// A persistent attachment to semantic surface coordinates. Authored control
/// positions remain unchanged; compilation resolves their position relative to
/// the captured surface frame, so repeated edits never accumulate drift.
public struct AnatomyBinding:Codable,Equatable,Sendable {
  public var id:String
  public var anchor:String
  public var target:String
  public var kind:AnatomyBindingKind
  public var bindPosition:V3
  public var bindNormal:V3
  public var followNormal:Bool
  public init(id:String,anchor:String,target:String,kind:AnatomyBindingKind,bindPosition:V3,bindNormal:V3,followNormal:Bool=false) {
    self.id=id;self.anchor=anchor;self.target=target;self.kind=kind
    self.bindPosition=bindPosition;self.bindNormal=bindNormal;self.followNormal=followNormal
  }
  public func validate(anchors:[AnatomyAnchor]) throws {
    try CraftError.require(!id.isEmpty && id.count<=80 && !target.isEmpty && target.count<=80
      && anchors.contains{$0.id==anchor} && CraftMath.finite(bindPosition,limit:200)
      && CraftMath.finite(bindNormal,limit:1) && (0.9...1.1).contains(length(bindNormal)),
      "anatomyBinding.\(id)","Use a current anchor, stable target ID and finite captured surface frame")
  }
  public func position(_ point:V3,anchors:[AnatomyAnchor])->V3 {
    guard let current=anchors.first(where:{$0.id==anchor}) else {return point}
    let relative=point-bindPosition
    let rotated=followNormal ? simd_quatf(from:normalize(bindNormal),to:normalize(current.sourceNormal)).act(relative):relative
    return current.sourcePosition+rotated
  }
  public func displacement(anchors:[AnatomyAnchor])->V3 {
    guard let current=anchors.first(where:{$0.id==anchor}) else {return .zero}
    return current.sourcePosition-bindPosition
  }
}

extension CreatureCraft {
  public func binding(_ kind:AnatomyBindingKind,_ target:String)->AnatomyBinding? {
    anatomyBindings.first{$0.kind==kind && $0.target==target}
  }
  public func boundPosition(_ point:V3,kind:AnatomyBindingKind,target:String)->V3 {
    binding(kind,target)?.position(point,anchors:anatomyAnchors) ?? point
  }
  public var boundGuideChains:[CraftGuideChain] {
    guideChains.map{chain in var c=chain;c.points=c.points.map{boundPosition($0,kind:.guideChain,target:c.id)};return c}
  }
  public var boundLandmarks:[AnatomyLandmark] {
    landmarks.map{landmark in var l=landmark;l.position=boundPosition(l.position,kind:.landmark,target:l.id);return l}
  }
  public var boundSkinFields:[SkinField] {
    skinFields.map{field in var f=field;f.center=boundPosition(f.center,kind:.skinField,target:f.id);return f}
  }
  public var boundCorrectives:[PoseCorrective] {
    correctives.map{corrective in var c=corrective;c.field.center=boundPosition(c.field.center,kind:.corrective,target:c.id);return c}
  }
  public func boundSurfaceLayers(_ layers:[SurfaceLayer])->[SurfaceLayer] {
    layers.map{layer in var l=layer;l.center=boundPosition(l.center,kind:.surfaceLayer,target:l.id);return l}
  }
}
