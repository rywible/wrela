import FieldCore
import FieldEngine
import Foundation
import simd

extension WorkshopRenderer {
  func secondaryPose(lab:CreatureWorkshop? = nil,player:SecondaryPosePlayer? = nil)->[String:simd_float4x4] {
    let lab=lab ?? creatureWorkshop,p=player ?? secondaryPlayer,rig=studioSource.resolvedSecondaryRig
    guard lab.mode != "bind",!rig.nodes.isEmpty else {
      var pose=lab.matrices(source:studioSource)
      for n in rig.nodes {pose[n.id]=pose[n.joint] ?? matrix_identity_float4x4}
      return pose
    }
    let encoder=JSONEncoder();encoder.outputFormatting=[.sortedKeys]
    let stimulus=(try? encoder.encode(lab.stimulusOverride)) ?? Data()
    let key=sourceRevision+"/\(lab.mode)/\(lab.seed)/\(lab.scenario)/\(lab.rehearsal)/\(lab.contactSolving)/"+stimulus.base64EncodedString()
    let simulated=p.evaluate(key:key,rig:rig,settings:studioSource.secondary,time:lab.seconds,sample:{time in
      var sample=lab
      if sample.mode=="behavior" {try? sample.seek(time,motion:self.studioSource.motion ?? MotionParameters())}
      else {sample.seconds=time}
      let root=sample.root(self.studioSource.motion ?? MotionParameters())
      return sample.matrices(source:self.studioSource).mapValues{root*$0}
    },colliders:studioSource.resolvedColliders,height:lab.rehearsal.height)
    let inverse=lab.root(studioSource.motion ?? MotionParameters()).inverse
    return simulated.mapValues{inverse*$0}
  }
  func secondaryReport(samples:Int) throws -> [String:Any] {
    guard (2...121).contains(samples) else {throw RigError.invalid}
    let player=SecondaryPosePlayer();var rows:[[String:Any]]=[]
    for i in 0..<samples {
      var lab=creatureWorkshop
      try lab.seek(performanceDuration*Float(i)/Float(samples-1),motion:studioSource.motion ?? MotionParameters())
      _=secondaryPose(lab:lab,player:player)
      rows.append(["time":lab.seconds,"metrics":try JSONSerialization.jsonObject(with:JSONEncoder().encode(player.metrics))])
    }
    return ["revision":sourceRevision,"nodes":studioSource.resolvedSecondaryRig.nodes.count,"patches":studioSource.resolvedSecondaryRig.patches.count,"samples":rows,
      "scope":"Fixed-step XPBD guides; particle contact and optional registered span/bilinear-patch capsule contact, sampled height-field contact and positional friction. Patch queries use bounded multi-start interior minimization and report residuals, including immovable pins. Pinned contacts are diagnosed, not moved. Proxies are not rendered triangles. No self-collision, swept CCD, global clearance proof for folded patches, or body reaction."]
  }
  func editSecondary(_ values:[String:Any]) throws {
    guard Set(values.keys).isSubset(of:["id","action","enabled","gravity","damping","followCompliance","wind","substeps","iterations","friction","edgeContacts"]) else {throw RigError.invalid}
    var candidate=studioSource
    var settings=try JSONSerialization.jsonObject(with:JSONEncoder().encode(candidate.secondary)) as! [String:Any]
    for (k,v) in values where !["id","action"].contains(k) {
      guard let n=v as? NSNumber,(["enabled","edgeContacts"].contains(k)) == (CFGetTypeID(n)==CFBooleanGetTypeID()) else {throw RigError.invalid}
      settings[k]=v
    }
    candidate.secondary=try JSONDecoder().decode(SecondarySettings.self,from:JSONSerialization.data(withJSONObject:settings))
    try candidate.validate();try applySource(candidate)
  }
}
