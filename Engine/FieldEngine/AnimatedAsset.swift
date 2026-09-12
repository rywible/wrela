import FieldCore
import simd

/// Runtime adapter for the exact asset source inspected in Soundstage.
/// This owns render resources and a preview clock, never a game's rules or saves.
package final class AnimatedAsset {
  private let secondaryPlayer = SecondaryPosePlayer()
  package let source: AssetSource
  package let batches: [GPUBatch]
  private let secondaryRig:SecondaryRig
  private let colliders:[RigCapsule]
  private let animation:AnimationDefinition?
  private let correctivesByBatch:[[PoseCorrective]]
  private let layersByBatch:[[SurfaceLayer]]
  package init(source: AssetSource, graphics: MetalRenderer) throws {
    self.source = source
    batches = try source.compile().map(graphics.upload)
    secondaryRig=source.resolvedSecondaryRig
    colliders=source.resolvedColliders
    animation=source.animation
    correctivesByBatch=batches.map{batch in source.resolvedCorrectives.filter{$0.field.part==batch.name}}
    layersByBatch=batches.map{batch in source.resolvedSurfaceLayers.filter{$0.part==batch.name}}
  }
  package func items(
    clip: String, time: Float, state: BehaviorSnapshot = BehaviorSnapshot(),
    placement: simd_float4x4 = matrix_identity_float4x4,
    terrain: ((Float,Float)->Float)? = nil
  ) -> [RenderItem] {
    func height(_ x:Float,_ z:Float)->Float {
      guard let terrain else {return 0}
      return ContactRig.localHeight(frame:placement,x:x,z:z,height:terrain)
    }
    func rootAt(_ t:Float)->simd_float4x4 {
      animation?.root(clip,t,state,source.motion ?? MotionParameters()) ?? matrix_identity_float4x4
    }
    let root=rootAt(time)
    let simulated = secondaryPlayer.evaluate(key:clip,rig:secondaryRig,settings:source.secondary,time:time,
      sample:{t in
        let root=rootAt(t)
        func localHeight(_ x:Float,_ z:Float)->Float {ContactRig.localHeight(frame:root,x:x,z:z,height:height)}
        return self.source.evaluatedPose(clip:clip,time:t,state:state,height:localHeight,
          normal:{x,z in ContactRig.surfaceNormal(x:x,z:z,height:localHeight)}).matrices.mapValues{root*$0}
      },colliders:colliders,height:height)
    let inverseRoot=root.inverse
    let matrices=simulated.mapValues{inverseRoot*$0}
    let correctivePose=clip != "bind" && !source.craft.correctives.isEmpty
      ? source.authoredPose(clip:clip,time:time,state:state).poses:[:]
    return batches.enumerated().map { index,batch in
      var instance = batch.sourceInstances[0]
      instance.model =
        placement * root
        * (batch.skinJoints.isEmpty
          ? matrices[batch.name] ?? matrix_identity_float4x4 : matrix_identity_float4x4)
        * instance.model
      var item = RenderItem(batch: batch, instance: instance)
      item.correctives=clip == "bind" ? []:correctivesByBatch[index].map {c in
        CorrectiveUniform(c.field,weight:c.activation(correctivePose[c.driver] ?? JointPose()))
      }
      item.surfaceLayers=layersByBatch[index]
      item.skinPalette = batch.skinJoints.map { matrices[$0] ?? matrix_identity_float4x4 }
      return item
    }
  }
}
