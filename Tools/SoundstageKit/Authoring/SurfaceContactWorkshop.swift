import FieldCore
import FieldCompiler
import FieldEngine
import Foundation
import simd

extension WorkshopRenderer {
  func surfaceContactReport(samples:Int,part:String? = nil) throws -> [String:Any] {
    guard (1...121).contains(samples) else {throw RigError.invalid}
    let rig=studioSource.resolvedSecondaryRig,ids=Set(rig.nodes.map(\.id))
    // Recompile authored Swift source, including local sculpt fields and guide
    // offsets. This is an explicit audit, never part of the live frame loop.
    let all=try studioSource.compile()
    let batches=all.filter {batch in part.map{$0==batch.name} ?? batch.skinJoints.contains(where:ids.contains)}
    guard !batches.isEmpty else {throw RuntimeError.message("Choose a valid part or an asset with skinned secondary surfaces")}
    let player=SecondaryPosePlayer();var rows:[[String:Any]]=[]
    var maximum:Float=0,worstPart="",worstTime:Float=0
    for i in 0..<samples {
      var lab=creatureWorkshop
      if samples>1 {try lab.seek(performanceDuration*Float(i)/Float(samples-1),motion:studioSource.motion ?? MotionParameters())}
      let pose=secondaryPose(lab:lab,player:player),root=lab.root(studioSource.motion ?? MotionParameters())
      let bodies=studioSource.resolvedColliders.map{$0.transformed(root*(pose[$0.joint] ?? matrix_identity_float4x4))}
      for batch in batches {
        for (instanceID,instance) in batch.instances.enumerated() {
          let rigid=batch.skinJoints.isEmpty ? pose[batch.name] ?? matrix_identity_float4x4:matrix_identity_float4x4
          let authored=studioSource.authoredPose(clip:lab.mode,time:lab.seconds,state:lab.actor).poses
          let fields=studioSource.resolvedCorrectives.filter{$0.field.part==batch.name}.map{c->SurfaceEdit in
            var field=c.field;let weight=lab.mode=="bind" ? 0:c.activation(authored[c.driver] ?? JointPose())
            field.offset *= weight;field.dilation *= weight;return field
          }
          let corrected=DeformationAudit.corrected(batch.mesh,fields:fields)
          let result=SkinContactAudit.evaluate(mesh:corrected,weights:batch.skinWeights,
            palette:batch.skinJoints.map{pose[$0] ?? matrix_identity_float4x4},model:root*rigid*instance.model,
            bodies:bodies,height:lab.rehearsal.height)
          var influences:[[String:Any]]=[]
          if result.vertex>=0 && batch.skinWeights.indices.contains(result.vertex) {
            let w=batch.skinWeights[result.vertex]
            for k in 0..<4 where w.weights[k]>0 {
              influences.append(["guide":batch.skinJoints[Int(w.joints[k])],"weight":w.weights[k]])
            }
          }
          rows.append(["time":lab.seconds,"part":batch.name,"instance":instanceID,"contact":try JSONSerialization.jsonObject(with:JSONEncoder().encode(result)),"influences":influences])
          if result.maximumPenetration>maximum {maximum=result.maximumPenetration;worstPart=batch.name;worstTime=lab.seconds}
        }
      }
    }
    return ["revision":sourceRevision,"maximumPenetration":maximum,"worstPart":worstPart,"worstTime":worstTime,"samples":rows,
      "toleranceMetres":0.0005,"scope":"All compiled vertices after pose correctives and four-weight skinning, against registered body capsules and the rehearsal height field. Positions are rehearsal metres. Default selection is every batch influenced by secondary guides. Intentional attachment overlap is included. No triangle-interior, self-contact, swept-time or shader-wind test; this is a diagnostic, not a surface collision solver."]
  }
}
