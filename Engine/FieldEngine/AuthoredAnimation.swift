import FieldCore
import Foundation
import simd

/// Compact source animation metadata reconstructed when editor studies load. The actual
/// authored poses/contact paths always evaluate from AssetSource.craft.
package struct AuthoredAnimation: Codable {
  package var clips:[String]
  package var duration:Float
  package var links:[PartJoint]
  package var nonloopingClip:String?
  package init(craft:CreatureCraft,joints:[PartJoint]) {
    clips=Array(Set(craft.phrases.map(\.id)+(craft.arrangement.map{[$0.clip]} ?? []))).sorted()
    duration=craft.arrangement?.duration ?? craft.phrases.first?.duration ?? 1
    links=joints
    nonloopingClip=craft.arrangement.flatMap{$0.looping ? nil:$0.clip}
  }
  package func resolve(_ base:AnimationDefinition?) -> AnimationDefinition? {
    guard !clips.isEmpty else {return base}
    var result=base ?? AnimationDefinition(controls:[],clips:[],scenarios:[],signalName:"Authored",
      duration:{_ in duration},poses:{_,_,_,_ in [:]},root:{_,_,_,_ in matrix_identity_float4x4},
      initialize:{_ in BehaviorSnapshot()},step:{state,_,_ in state},input:{_,_ in PreviewInput()},
      validate:{p in try CraftError.require(p.values.isEmpty,"motion","Source-authored clips have no generator motion controls")})
    result.clips += clips.filter{!result.clips.contains($0)}
    if base==nil {result.rigLinks=links.compactMap{joint in joint.parent.map{($0,joint.id)}}}
    return result
  }
}
